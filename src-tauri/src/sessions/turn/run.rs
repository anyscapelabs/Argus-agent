// The loop itself. `Turn` and its mutable state are in `mod.rs`; the steps that
// surround the loop — building the request, watching the budget, classifying
// the reply, persisting it — are one file each.
//
// The split is by decision, not by length: each helper answers exactly one
// question the loop asks, and none of them can reach the loop's locals. What is
// left here is the part that genuinely has to be read top to bottom.
use rusqlite::params;
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager};

use super::{Truncation, Turn};
use crate::gateway::router;
use crate::gateway::schema::StreamEvent;
use crate::gateway::Gateway;
use crate::sessions::chat::{
    approval_id, ask_approval, run_skill_reflection, DENIED_CODE, EMPTY_CONT, HARD_STEPS,
    MAX_CLAIM_NUDGES, MAX_STEPS, NUDGE, RESULT_CLIP, SUMMARY_DEMAND,
};
use crate::sessions::schema::NewMsg;
use crate::sessions::{blocks, guards, reflect, sink, store};
use crate::tools;
use crate::tools::ToolCallStyle;
/// Run the loop to completion. `perm`, `web`, `allow_hosts`, and
/// `turn_budget` are fixed for the whole turn, so they arrive as
/// parameters; everything the loop itself changes is on `self`.
#[allow(clippy::too_many_arguments)]
impl Turn {
    pub async fn run<R: tauri::Runtime>(
        &mut self,
        gw: &Gateway,
        app: &AppHandle<R>,
        session_id: &str,
        sink: &dyn sink::ChatSink,
        model_chan: &Channel<StreamEvent>,
        perm: &str,
        web: bool,
        allow_hosts: &[String],
        turn_budget: i64,
    ) -> Result<(), String> {
        for _step in 0..MAX_STEPS {
            let req = self.build_request(gw, session_id)?;

            // Checked before the call, not after: at Stop the turn is over and the
            // model never gets to spend what is left of the budget.
            if self.budget_stopped(gw, session_id, sink, turn_budget) {
                break;
            }

            if !self.budget_warned
                && guards::budget_state(self.tok_in_sum, turn_budget) == guards::Budget::Warn
            {
                // A contract break already queued its own correction and is more
                // urgent than a heads-up, so the budget warning waits one step
                // rather than overwriting it. Latched only once actually queued,
                // or it would be lost and never re-fires.
                if self.nudge.is_none() {
                    self.budget_warned = true;
                    self.nudge = Some(format!(
                        "You have used about {pct}% of this turn's budget. Finish the task now, \
                 or report what is done and what is left in one answer. Close it with \
                 <final/>.",
                        pct = (guards::BUDGET_WARN_FRACTION * 100.0) as u32
                    ));
                }
            }

            let stats = router::stream_run(gw, req, model_chan).await?;
            self.tok_in_sum += stats.tok_in;

            if stats.text.trim().is_empty() && stats.tool_calls.is_empty() {
                if self.empty_retries < 1 {
                    self.empty_retries += 1;
                    self.nudge = Some(EMPTY_CONT.into());
                    // Whitespace arrived and is being thrown away, so it is
                    // never persisted. Nothing downstream will clear it, and
                    // the retry streams onto whatever is still in the buffer.
                    sink.emit(StreamEvent::Reset);
                    continue;
                }

                return Err("model returned an empty reply — try again".into());
            }

            let normalized = blocks::sanitize_tags(&tools::normalize_actions(&stats.text));
            let (closed, base_text) = tools::split_commit(&normalized);
            // Degraded replies have no API channel, so the text must execute
            // regardless of style. Otherwise the resolved style decides: native
            // prose is never executed, template text is decoded.
            let style = Turn::resolve_style(gw, &stats, &base_text);
            let mut pending =
                tools::build_executions_styled(&base_text, &stats.tool_calls, self.act_base, style);

            let done = pending.is_empty();
            // Native turns persist prose only: the event rows own what ran, so no
            // record markup is stored to be re-parsed later. Degraded and template
            // turns keep the text blocks — they are the only record those have.
            let mut text = if style == ToolCallStyle::Native && !stats.degraded {
                tools::strip_actions(&base_text)
            } else {
                tools::render_actions(&base_text, &pending)
            };

            let calls_json = if stats.tool_calls.is_empty() {
                None
            } else {
                serde_json::to_string(&stats.tool_calls).ok()
            };

            self.record_observations(gw, session_id, style, &stats);

            let model_id = stats.model_id.clone();
            let (asst, said_again) =
                self.persist_assistant(gw, session_id, &text, &model_id, &stats, calls_json)?;

            // The step boundary, and it belongs to the persist rather than to
            // the work that follows it. `Step` is what tells the frontend its
            // text buffer is spent, and this is the instant that becomes true.
            //
            // Emitted further down, it only covered the paths that run tools.
            // A nudge, a truncation retry or a reflection all `continue` past
            // that point, so the buffer was never cleared and the next step
            // streamed on top of a reply that was already a row — the same
            // words shown twice, once from the row and once from the buffer.
            sink.emit(StreamEvent::Step);

            let mut trunc_overflow = false;
            match self.handle_truncation(gw, &asst, sink, &stats, done, said_again) {
                Truncation::Finish => break,
                Truncation::Retry => continue,
                Truncation::Overflow => trunc_overflow = true,
                Truncation::None => {}
            }

            if !stats.truncated && done {
                // Asked of what the model actually wrote, not of the text the
                // transcript shows: a fragment too broken to run is cut out of the
                // answer, and the model still has to be told to say it again.
                let orphaned = tools::has_orphaned_action_block(&stats.text);

                if !closed
                    || reflect::fakes_output(&text)
                    || reflect::has_faux_sandbox(&text)
                    || orphaned
                {
                    if self.claim_nudges < MAX_CLAIM_NUDGES {
                        self.claim_nudges += 1;
                        self.nudge = Some(NUDGE.into());
                        continue;
                    }

                    if !self.forced_summary {
                        self.forced_summary = true;
                        self.nudge = Some(SUMMARY_DEMAND.into());
                        continue;
                    }

                    sink.emit(StreamEvent::Notice {
                        msg: "the reply described an action but none ran — partial work \
                      above is saved; send 'continue' to let it retry"
                            .into(),
                    });

                    if let Ok(conn) = gw.conn.lock() {
                        let _ = conn.execute(
                            "UPDATE messages SET content = content || ?2 WHERE id = ?1",
                            params![
                                &asst.id,
                                "\n<warning severity=\"medium\">this turn described actions that \
                         never ran — the work above is saved; send 'continue' to let it \
                         retry</warning>"
                            ],
                        );
                        let _ = store::mark_final(&conn, &asst.id);
                    }
                    // "Send continue to retry" is a promise, so the retry has to
                    // find out what was attempted.
                    blocks::save_resume(gw, session_id, &self.turn_actions);

                    self.finished = true;
                    break;
                }

                if self.acts_run >= HARD_STEPS {
                    let app2 = app.clone();
                    let sid = session_id.to_string();
                    tauri::async_runtime::spawn(async move {
                        run_skill_reflection(&app2, &sid).await;
                    });
                }

                // A self.finished turn closes its own resume: leaving one behind would
                // tell the next turn there is unfinished work that is not.
                if let Ok(conn) = gw.conn.lock() {
                    crate::sessions::resume::clear(&conn, session_id);
                }

                let reflect_on: bool = gw
                    .conn
                    .lock()
                    .ok()
                    .and_then(|conn| {
                        conn.query_row(
                            "SELECT reflect FROM sessions WHERE id = ?1",
                            params![session_id],
                            |r| r.get::<_, i64>(0),
                        )
                        .ok()
                    })
                    .map(|v| v != 0)
                    .unwrap_or(false);

                if reflect::should_reflect(reflect_on, self.acts_run, self.reflect_nudges) {
                    match reflect::run_reflection_check(gw, session_id, &text).await {
                        Ok(None) => {
                            if let Ok(conn) = gw.conn.lock() {
                                let _ = conn.execute(
                                    "UPDATE messages SET content = content || ?2 WHERE id = ?1",
                                    params![&asst.id, format!("\n{}", reflect::check_block(true))],
                                );
                            }
                        }
                        Ok(Some(instruction)) => {
                            self.reflect_nudges += 1;

                            if let Ok(conn) = gw.conn.lock() {
                                let _ = conn.execute(
                                    "UPDATE messages SET content = content || ?2 WHERE id = ?1",
                                    params![&asst.id, format!("\n{}", reflect::check_block(false))],
                                );
                            }

                            self.nudge = Some(instruction);
                            continue;
                        }
                        Err(_) => {}
                    }
                }

                {
                    let app3 = app.clone();
                    let sid = session_id.to_string();
                    let mid = model_id.clone();
                    tauri::async_runtime::spawn(async move {
                        let app4 = app3.clone();
                        let gw = app4.state::<Gateway>();
                        crate::learning::learn_pending(&gw).await;

                        // Curating is a lookup and an upsert per kind, with the
                        // sentences fixed in code — no model call, so it cannot
                        // stall a turn or cost anything.
                        let conn = gw.conn.lock().ok();
                        if let Some(conn) = conn {
                            let _ = crate::playbook::curate(&conn, &mid, Some(&sid));
                            let _ = crate::playbook::curate(
                                &conn,
                                &crate::sessions::ext_install::host_id(),
                                Some(&sid),
                            );
                        }
                    });
                }

                if let Ok(conn) = gw.conn.lock() {
                    let _ = store::mark_final(&conn, &asst.id);
                }

                self.finished = true;
                break;
            }

            let mut edits: Vec<(usize, usize, String)> = vec![];
            let mut append_blocks: Vec<String> = vec![];
            let mut shown_candidates: Vec<(String, &'static str, Option<u64>)> = vec![];

            let mut events_ok = true;

            for exec in pending.iter_mut() {
                let idx: usize = exec
                    .id
                    .strip_prefix('a')
                    .and_then(|n| n.parse().ok())
                    .unwrap_or(self.act_base);
                if idx >= self.act_base {
                    self.act_base = idx + 1;
                }

                let is_term = exec.is_terminal_tool();
                let is_browser = exec.is_browser_tool();

                let args_v: serde_json::Value =
                    serde_json::from_str(&exec.args).unwrap_or(serde_json::Value::Null);

                let cmd = if is_term {
                    args_v["command"].as_str().unwrap_or_default().to_string()
                } else {
                    String::new()
                };

                let pre_failed = exec.status.is_terminal();
                let mut denied = false;
                // Set on the gate path below, read by the single observation site
                // after it. A pre-failed exec never trips the guard, so it starts
                // false rather than reading a stale value from the last exec.
                let mut thrashed = false;
                let code: i64;

                if pre_failed {
                    code = -1;
                    // Recorded here because the gate below is skipped, but the
                    // outcome itself is pushed once for every exec further down,
                    // so both histories stay aligned and failure-first.
                    self.recent.push((exec.tool.clone(), exec.args.clone()));
                } else {
                    if exec.tool_call_id.is_some() {
                        sink.emit(StreamEvent::Delta {
                            text: format!("<action tool=\"{}\">{}</action>", exec.tool, exec.args),
                        });
                    }

                    exec.begin();

                    let sensitive =
                        is_browser && tools::browser::sensitive(&exec.tool, &args_v).await;
                    let needs_ask = (perm == "ask" && tools::is_mutating(&exec.tool)) || sensitive;

                    let key = (exec.tool.clone(), exec.args.clone());
                    let looped = guards::repeated(&self.recent, &key);
                    thrashed = guards::thrashing(&self.recent, &self.recent_out, &key);
                    self.recent.push(key);

                    // A guard trip is not an approval question, so it never opens
                    // the Run/Deny card: showing it and then failing the call
                    // anyway asks the user to approve something already refused.
                    // The user is the escape hatch, and the guard is advisory.
                    let mut allow = !needs_ask && !looped && !thrashed;
                    let needs_ask = needs_ask && !looped && !thrashed;

                    if !allow && (looped || thrashed) {
                        sink.emit(StreamEvent::Notice {
                            msg: if thrashed {
                                "skipped a step that retried the same failing approach — say what to \
                         try instead, or run it yourself"
                            } else {
                                "skipped a step that repeats the same call — say what to try \
                         instead, or run it yourself"
                            }
                            .into(),
                        });
                    }

                    if needs_ask {
                        let what = if is_browser {
                            blocks::browser_what(&exec.tool, &args_v, false)
                        } else if exec.tool == "doc.create" {
                            args_v
                                .get("name")
                                .and_then(|v| v.as_str())
                                .map(|s| format!("doc.create {}", s))
                                .unwrap_or_else(|| "doc.create".into())
                        } else {
                            cmd.clone()
                        };

                        let mut edited: Option<String> = None;

                        if sink.detached() {
                            sink.emit(StreamEvent::Notice {
                                msg: format!(
                                    "skipped a step that needs your approval ({what}) — nothing was \
                             listening, so it was refused rather than guessed at. Open the \
                             session and ask again, or set it to never"
                                ),
                            });
                            allow = false;
                            denied = true;
                        } else {
                            let reply =
                                ask_approval(gw, sink, &approval_id(), idx as u32, &what).await;
                            edited = reply.args;
                            allow = reply.allow;
                            denied = !allow;
                        }

                        if allow && !exec.is_browser_tool() {
                            if let Some(args) = edited {
                                exec.args = args;
                            }
                        }

                        if denied && is_term {
                            sink.emit(StreamEvent::TermEnd {
                                idx: idx as u32,
                                code: DENIED_CODE,
                            });
                        }
                    }

                    if looped || thrashed {
                        exec.fail(
                    if thrashed {
                        "retried the same approach without progress — change approach or ask the user"
                    } else {
                        "same action 3 times without visible progress — change approach or ask the user"
                    }
                    .to_string(),
                );
                        code = -1;
                    } else if denied {
                        exec.cancel("action denied by user".to_string());
                        code = DENIED_CODE;
                    } else {
                        let t0 = std::time::Instant::now();
                        let outcome = tools::recover::exec_with_recovery(tools::ExecIn {
                            app,
                            gw,
                            name: &exec.tool,
                            args_json: &exec.args,
                            permission: perm,
                            web,
                            approved: allow,
                            on_term: sink.term_chan().map(|c| (c, idx as u32)),
                        })
                        .await;
                        exec.elapsed_ms = t0.elapsed().as_millis();
                        match outcome.result {
                            Ok(t) => {
                                code = blocks::exit_of(&t);
                                exec.succeed(t);
                            }
                            Err(err) => {
                                if exec.is_browser_tool()
                                    && err.contains("Chrome is not connected to Argus")
                                {
                                    sink.emit(StreamEvent::Notice {
                                        msg: "the agent needs your real Chrome once: open \
                                    chrome://extensions, enable Developer mode, click \
                                    Load unpacked and pick the Argus extension folder, \
                                    then tell it to try again"
                                            .into(),
                                    });
                                }

                                exec.fail(err);
                                code = -1;
                            }
                        }
                    }
                }

                debug_assert!(
                    exec.status.is_terminal(),
                    "tool execution must end terminal: {}",
                    exec.tool
                );
                let status = exec.result_status();
                let body = exec.result_body().to_string();
                // Outcome for the semantic guard. One push per exec, on every path
                // including pre-failed ones that never ran: the caller pushes to
                // `self.recent` unconditionally too, and the two must stay the same
                // length for the window slices to mean anything.
                let exec_failed = exec.status == tools::ToolStatus::Failed
                    || exec.status == tools::ToolStatus::Cancelled;
                self.recent_out.push(exec_failed);
                self.turn_actions
                    .push((blocks::exec_label(exec, is_term), !exec_failed));

                // A failure is the one moment worth learning from, so it is
                // observed here rather than at each site that could fail. A
                // dropped signal costs a lesson; a spurious one costs prompt
                // budget, and neither is worth a turn's outcome.
                if exec_failed {
                    if let Ok(conn) = gw.conn.lock() {
                        blocks::observe_exec(&conn, &stats.model_id, exec, thrashed);
                    }
                }

                if is_browser {
                    // Extract the generation now; the body itself is never
                    // needed again, so it is not cloned into the candidate.
                    let gen = tools::browser::shown_gen_in(&body);
                    shown_candidates.push((exec.args.clone(), status, gen));
                }
                let msg = exec.to_tool_result(RESULT_CLIP);

                {
                    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
                    store::add_msg(
                        &conn,
                        &NewMsg {
                            session_id: session_id.into(),
                            role: "user".into(),
                            content: msg,
                            model_id: None,
                            provider_id: None,
                            tok_in: None,
                            tok_out: None,
                            tool_calls: None,
                            tool_call_id: exec.tool_call_id.clone(),
                            attachments: None,
                        },
                    )?;
                }

                // The structured twin of the text block below. Same execution, so
                // the card, the history, and the audit can never disagree. A lost
                // row falls back to text (the turn must never fail over logging)
                // and is counted in connector_logs under service `sessions`.
                let ev = crate::sessions::events::from_execution(exec, &asst.id, session_id);
                let ev_err = match gw.conn.lock() {
                    Ok(conn) => store::add_event(&conn, &ev).err(),
                    Err(err) => Some(err.to_string()),
                };
                if let Some(err) = ev_err {
                    events_ok = false;
                    crate::connectors::log::event(
                        "sessions",
                        "tool_event_write",
                        &format!("{}: {err}", exec.tool),
                        "err",
                    );
                }

                if is_term {
                    let terminal_failed = exec.status == tools::ToolStatus::Failed;
                    if status == "ok" || terminal_failed {
                        let term_code = if denied { DENIED_CODE } else { code };
                        sink.emit(StreamEvent::TermEnd {
                            idx: idx as u32,
                            code: term_code,
                        });
                    }

                    let out = if denied || exec.status == tools::ToolStatus::Cancelled {
                        "command denied by user".to_string()
                    } else {
                        body.strip_prefix("exit ")
                            .and_then(|r| r.split_once('\n'))
                            .map(|(_, o)| o.to_string())
                            .unwrap_or_else(|| body.clone())
                    };

                    let blk = blocks::terminal_block(idx, &cmd, code, &out, exec.elapsed_ms);
                    match (exec.start, exec.end) {
                        (Some(s), Some(e)) => edits.push((s, e, blk)),
                        _ => append_blocks.push(blk),
                    }
                }

                if is_browser {
                    let url = args_v["url"]
                        .as_str()
                        .map(Into::into)
                        .unwrap_or_else(|| blocks::body_url(&body));

                    let blk = blocks::browser_block(
                        idx,
                        &exec.tool,
                        &url,
                        &blocks::browser_what(&exec.tool, &args_v, true),
                    );
                    match (exec.start, exec.end) {
                        (Some(s), Some(e)) => edits.push((s, e, blk)),
                        _ => append_blocks.push(blk),
                    }
                }

                if exec.tool == "doc.create" && status == "ok" {
                    let id = blocks::doc_field(&body, "id=");
                    let name = blocks::doc_field(&body, "name=");
                    let ext = blocks::doc_field(&body, "ext=");
                    let pages = blocks::doc_field(&body, "pages=");
                    let title = if name.is_empty() {
                        "Untitled document".into()
                    } else {
                        name
                    };
                    let blk = blocks::doc_block(&id, &title, &ext, &pages);
                    match (exec.start, exec.end) {
                        (Some(s), Some(e)) => edits.push((s, e, blk)),
                        _ => append_blocks.push(blk),
                    }
                }

                if exec.tool == "code.run" {
                    let cmd = args_v
                        .get("command")
                        .and_then(|v| v.as_str())
                        .unwrap_or("command");
                    let blk = crate::tools::sandbox::record_block(
                        cmd,
                        crate::tools::sandbox::Profile::Restricted.as_str(),
                        &crate::tools::sandbox::origin_label(
                            self.turn_origin.as_ref(),
                            allow_hosts,
                        ),
                        status,
                        &body,
                    );
                    match (exec.start, exec.end) {
                        (Some(s), Some(e)) => edits.push((s, e, blk)),
                        _ => append_blocks.push(blk),
                    }
                }

                if exec.tool != "code.run" {
                    if let Some(o) = crate::tools::sandbox::origin_of_tool(&exec.tool, &exec.args) {
                        self.turn_origin = Some(o);
                    }
                }

                if exec.tool_call_id.is_some()
                    && !is_term
                    && !is_browser
                    && exec.tool != "doc.create"
                {
                    append_blocks.push(format!(
                        "<action tool=\"{}\">{}</action>",
                        exec.tool, exec.args
                    ));
                }
            }

            // Native turns keep prose only: events own the records (written
            // above), so splicing text blocks would resurrect the markup the
            // structured path exists to delete. A lost event row falls back to
            // text so the work stays visible. Degraded turns have no events
            // worth reading, so their text blocks stay.
            if (!edits.is_empty() || !append_blocks.is_empty())
                && blocks::needs_text_blocks(style, stats.degraded, events_ok)
            {
                // `text` is dead after this splice (reflection already read it
                // above), so take it instead of cloning the whole transcript.
                let mut updated = std::mem::take(&mut text);

                for (s, end, blk) in edits.into_iter().rev() {
                    if s <= end
                        && end <= updated.len()
                        && updated.is_char_boundary(s)
                        && updated.is_char_boundary(end)
                    {
                        updated.replace_range(s..end, &blk);
                    }
                }
                for blk in append_blocks {
                    if !updated.is_empty() && !updated.ends_with('\n') {
                        updated.push('\n');
                    }
                    updated.push_str(&blk);
                }

                let conn = gw.conn.lock().map_err(|err| err.to_string())?;
                conn.execute(
                    "UPDATE messages SET content = ?2 WHERE id = ?1",
                    params![&asst.id, &updated],
                )
                .map_err(|err| err.to_string())?;
            }

            for (args_json, status, gen) in shown_candidates {
                if status != "ok" {
                    continue;
                }
                let Some(gen) = gen else {
                    continue;
                };
                let Ok(args_v) = serde_json::from_str::<serde_json::Value>(&args_json) else {
                    continue;
                };
                tools::browser::note_shown(&args_v, gen).await;
            }

            self.acts_run += pending.len();

            if trunc_overflow {
                sink.emit(StreamEvent::Notice {
                    msg: "the model's reply was cut off at its output limit twice — \
                  partial work above is saved; send 'continue' to resume"
                        .into(),
                });
                blocks::save_resume(gw, session_id, &self.turn_actions);
                self.finished = true;
                break;
            }
        }

        if !self.finished {
            sink.emit(StreamEvent::Notice {
                msg: format!(
                    "paused mid-task after {MAX_STEPS} steps — everything above is saved; \
             send 'continue' to resume"
                ),
            });
            blocks::save_resume(gw, session_id, &self.turn_actions);
        }

        Ok(())
    }
}
