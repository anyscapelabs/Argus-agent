use rusqlite::params;
use tauri::{AppHandle, Manager};

use super::{Truncation, Turn};
use crate::gateway::router;
use crate::gateway::schema::StreamEvent;
use crate::gateway::{EventSink, Gateway};
use crate::sessions::chat::{
    approval_id, ask_approval, run_skill_reflection, DENIED_CODE, EMPTY_CONT, HARD_STEPS,
    MAX_CLAIM_NUDGES, MAX_STEPS, NUDGE, RESULT_CLIP, SUMMARY_DEMAND,
};
use crate::sessions::schema::NewMsg;
use crate::sessions::{blocks, guards, reflect, sink, store};
use crate::tools;
use crate::tools::ToolCallStyle;
/// Run the loop to completion. Turn-wide policy arrives as parameters.
#[allow(clippy::too_many_arguments)]
impl Turn {
    pub async fn run<R: tauri::Runtime>(
        &mut self,
        gw: &Gateway,
        app: &AppHandle<R>,
        session_id: &str,
        sink: &dyn sink::ChatSink,
        model_chan: &EventSink,
        perm: &str,
        web: bool,
        allow_hosts: &[String],
        turn_budget: i64,
    ) -> Result<(), String> {
        for _step in 0..MAX_STEPS {
            let req = self.build_request(gw, session_id)?;

            // Before the call, not after: at Stop the turn is over.
            if self.budget_stopped(gw, session_id, sink, turn_budget) {
                break;
            }

            if !self.budget_warned
                && guards::budget_state(self.tok_in_sum, turn_budget) == guards::Budget::Warn
            {
                // Latched: it fires once, and never over a contract break's own
                // correction.
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
                    // Whitespace is never persisted, so the retry would stream
                    // on top of it.
                    sink.emit(StreamEvent::Reset);
                    continue;
                }

                return Err("model returned an empty reply — try again".into());
            }

            let normalized = blocks::sanitize_tags(&tools::normalize_actions(&stats.text));
            let (closed, base_text) = tools::split_commit(&normalized);
            // Degraded replies have no API channel, so the text runs anyway.
            let style = Turn::resolve_style(gw, &stats, &base_text);
            let mut pending =
                tools::build_executions_styled(&base_text, &stats.tool_calls, self.act_base, style);

            let done = pending.is_empty();
            // Native turns persist prose only; event rows own what ran.
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

            // `Step` says the text buffer is spent. Emitted here, not below:
            // continue paths skip it, so the next step would stream on top of
            // a reply that is already a row.
            sink.emit(StreamEvent::Step);

            let mut trunc_overflow = false;
            match self.handle_truncation(gw, &asst, sink, &stats, done, said_again) {
                Truncation::Finish => break,
                Truncation::Retry => continue,
                Truncation::Overflow => trunc_overflow = true,
                Truncation::None => {}
            }

            if !stats.truncated && done {
                // Asked of the model's raw text: an unrunnable fragment is cut
                // out, so the model must say it again.
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
                    // "Send continue" is a promise; the retry needs the history.
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

                // A leftover resume claims unfinished work that is not.
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

                        // No model call, so it cannot stall a turn.
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
            // Owned: a per-exec temporary would not outlive the call.
            let ev = sink.event_sink();

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
                // A pre-failed exec never trips the guard, so it starts false
                // rather than reading a stale value from the last exec.
                let mut thrashed = false;
                let code: i64;

                if pre_failed {
                    code = -1;
                    // The gate is skipped but the history push is not, so push
                    // here to keep both aligned.
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

                    // A guard trip never opens the Run/Deny card. The user is
                    // the escape hatch, not the guard.
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
                            on_term: ev.as_ref().map(|c| (c, idx as u32)),
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
                // One push per exec, every path, so it stays the same length
                // as `self.recent`.
                let exec_failed = exec.status == tools::ToolStatus::Failed
                    || exec.status == tools::ToolStatus::Cancelled;
                self.recent_out.push(exec_failed);
                self.turn_actions
                    .push((blocks::exec_label(exec, is_term), !exec_failed));

                // One site: a dropped signal costs a lesson, a spurious one
                // costs prompt budget.
                if exec_failed {
                    if let Ok(conn) = gw.conn.lock() {
                        blocks::observe_exec(&conn, &stats.model_id, exec, thrashed);
                    }
                }

                if is_browser {
                    // Pull the generation now; the body is not needed again.
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

                // The structured twin of the text block below. A lost row falls
                // back to text — the turn must never fail over logging.
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

            // Splicing text blocks into a native turn resurrects markup the
            // structured path exists to delete.
            if (!edits.is_empty() || !append_blocks.is_empty())
                && blocks::needs_text_blocks(style, stats.degraded, events_ok)
            {
                // `text` is dead after this splice (reflection read it above).
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
