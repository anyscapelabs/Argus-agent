// The dispatch table: a tool name goes in, its output comes out.
//
// One long `match`, in the order the arms are cheap to reject. Most names are
// refused by the first arm; only the ones that match reach a real
// implementation. Approval and the `mutating` table are the gate above this
// file, not in it — by the time a name gets here it has already been allowed.

use tauri::ipc::Channel;

use crate::gateway::schema::StreamEvent;

use super::*;

pub async fn exec<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    gw: &crate::gateway::Gateway,
    name: &str,
    args_json: &str,
    permission: &str,
    web: bool,
    approved: bool,
    on_term: Option<(&Channel<StreamEvent>, u32)>,
) -> Result<String, String> {
    let meta = TOOLS
        .iter()
        .chain(WEB_TOOLS.iter())
        .chain(browser::META.iter())
        .chain(notepad::META.iter())
        .chain(profile::META.iter())
        .chain(fs::read::META.iter())
        .chain(library::META.iter())
        .chain(connector::META.iter())
        .chain(conn_oauth::META.iter())
        .find(|t| t.name == name)
        .ok_or_else(|| format!("unknown tool {name}"))?;

    // Depth one, enforced where it counts. A sub-agent that could fan out
    // would multiply without anything in the way counting it.
    if name.starts_with("agent.") {
        let sid = notepad::current_session()
            .ok_or("a sub-agent may only be started from inside a conversation")?;

        let conn = gw.conn.lock().map_err(|e| e.to_string())?;
        let child = crate::sessions::store::is_child(&conn, &sid)?;

        if child {
            return Err("a sub-agent cannot start another sub-agent".into());
        }

        drop(conn);
    }

    if name.starts_with("web.") && !web {
        return Err(
            "web search is off for this session; the user can enable it from the + menu".into(),
        );
    }

    if meta.mutating && permission == "ask" && !approved {
        return Err("blocked: this session asks before acting; switch its permission to never to allow writes".into());
    }

    let args: Value =
        serde_json::from_str(args_json.trim()).map_err(|_| "action body is not valid JSON")?;

    match name {
        "terminal" | "bash.run" => {
            let profile = sandbox::parse_profile(&args, sandbox::Profile::Host)
                .map_err(|err| err.to_string())?;
            let origin = sandbox::origin_of_tool(name, args_json);
            let command = args["command"].as_str().ok_or("terminal needs a command")?;
            let elevated = args.get("privilege").and_then(|v| v.as_str()) == Some("admin");

            if args.get("background").and_then(|v| v.as_bool()) == Some(true) {
                let sid = crate::tools::notepad::current_session();

                let job = crate::jobs::spawn(
                    app,
                    gw,
                    crate::jobs::Spec {
                        session_id: sid.clone(),
                        command: command.to_string(),
                        cwd: args["cwd"].as_str().map(str::to_string),
                        profile,
                        privileged: elevated,
                        permission: permission.to_string(),
                        label: args["label"]
                            .as_str()
                            .filter(|s| !s.trim().is_empty())
                            .unwrap_or(command)
                            .chars()
                            .take(120)
                            .collect(),
                        wake: args.get("wake").and_then(|v| v.as_bool()).unwrap_or(true),
                        timeout_secs: args["timeout"].as_u64(),
                    },
                )?;

                return Ok(format!(
                    "started in the background as job {}. It is running now and you do not \
                     need to wait for it. Check job.list for its state and job.read for what \
                     it printed. When it finishes you will be told, in this same session, \
                     with the tail of its output — carry on with other work in the meantime.",
                    job.id
                ));
            }

            let out = sandbox::run(
                gw,
                sandbox::Request {
                    tool: name,
                    command,
                    profile,
                    cwd: args["cwd"].as_str(),
                    elevated,
                    permission,
                    timeout_secs: args["timeout"].as_u64(),
                    origin: origin.as_ref(),
                    background: false,
                    log: None,
                },
                on_term,
            )
            .await
            .map_err(|err| err.to_string())?;

            Ok(format!("exit {}\n{}", out.exit, out.combined()))
        }
        "job.list" => {
            let sid = args.get("session_id").and_then(|v| v.as_str());
            let conn = gw.conn.lock().map_err(|e| e.to_string())?;
            let jobs = crate::jobs::list(&conn, sid, args["limit"].as_u64().unwrap_or(20) as i64)?;

            if jobs.is_empty() {
                return Ok("no background jobs".into());
            }

            Ok(jobs
                .iter()
                .map(|j| {
                    format!(
                        "{} | {} | {} | exit {:?} | {}",
                        j.id,
                        j.state,
                        j.label,
                        j.exit,
                        crate::tools::clip_ends(j.command.clone())
                    )
                })
                .collect::<Vec<_>>()
                .join("\n"))
        }
        "job.read" => {
            let id = args["id"].as_str().ok_or("job.read needs an id")?;
            let max = args["max_chars"]
                .as_u64()
                .unwrap_or(8_000)
                .clamp(200, 60_000) as usize;
            let conn = gw.conn.lock().map_err(|e| e.to_string())?;
            let job = crate::jobs::get(&conn, id)?;

            drop(conn);

            let body = crate::jobs::tail(&crate::jobs::log_path(gw, id), max)?;

            Ok(format!(
                "job {} — {} — exit {:?} — {}\n{}",
                job.id,
                job.state,
                job.exit,
                job.label,
                if body.trim().is_empty() {
                    "(it printed nothing)".to_string()
                } else {
                    body
                }
            ))
        }
        "job.kill" => {
            let id = args["id"].as_str().ok_or("job.kill needs an id")?;

            if crate::jobs::kill(gw, id)? {
                Ok(format!("job {id} is being stopped"))
            } else {
                Ok(format!("job {id} was not running"))
            }
        }
        "agent.spawn" => {
            let sid = notepad::current_session()
                .ok_or("a sub-agent may only be started from inside a conversation")?;
            let prompt = args["prompt"]
                .as_str()
                .filter(|p| !p.trim().is_empty())
                .ok_or("agent.spawn needs a prompt")?;
            let name = args["name"].as_str().unwrap_or("sub-agent").to_string();
            let title = args["title"]
                .as_str()
                .filter(|t| !t.trim().is_empty())
                .unwrap_or(&prompt.chars().take(90).collect::<String>())
                .to_string();

            let (model_id, perm) = {
                let conn = gw.conn.lock().map_err(|e| e.to_string())?;
                let s = crate::sessions::store::get_session(&conn, &sid)?;
                (s.model_id, s.permission)
            };

            let run = crate::agents::spawn(
                app,
                gw,
                crate::agents::Spec {
                    parent_id: sid.clone(),
                    name,
                    title,
                    prompt: prompt.to_string(),
                    model_id,
                    permission: perm,
                    wake: args["wake"].as_bool().unwrap_or(true),
                },
            )?;

            // The card is a message of its own. Left inside this tool result
            // it would be rendered as a work step and never drawn at all.
            crate::sessions::chat::post(
                gw,
                &sid,
                "assistant",
                &format!(
                    "<agent id=\"{id}\" name=\"{name}\" state=\"running\">\n{title}\n</agent>",
                    id = run.id,
                    name = crate::sessions::blocks::esc_attr(&run.name),
                    title = crate::sessions::blocks::esc_attr(&run.title),
                ),
            );

            return Ok(format!(
                "sub-agent {} is running as \"{}\". Do not wait for it and do not \
                 start the same work again. Its card is in this chat; its answer \
                 arrives here when it finishes.",
                run.id, run.name
            ));
        }
        "agent.list" => {
            let sid = notepad::current_session().ok_or("no conversation to list sub-agents of")?;
            let conn = gw.conn.lock().map_err(|e| e.to_string())?;
            let runs = crate::agents::list(&conn, &sid)?;

            if runs.is_empty() {
                return Ok("this conversation has not started any sub-agents".into());
            }

            Ok(runs
                .iter()
                .map(|r| {
                    format!(
                        "{} | {} | {} | {}",
                        r.id,
                        r.state,
                        r.name,
                        r.result
                            .as_deref()
                            .unwrap_or("still working")
                            .chars()
                            .take(200)
                            .collect::<String>()
                    )
                })
                .collect::<Vec<_>>()
                .join("\n"))
        }
        "agent.read" => {
            let id = args["id"].as_str().ok_or("agent.read needs an id")?;
            let max = args["max_chars"]
                .as_u64()
                .unwrap_or(8_000)
                .clamp(200, 60_000) as usize;
            let conn = gw.conn.lock().map_err(|e| e.to_string())?;
            let run = crate::agents::get(&conn, id)?;
            drop(conn);

            let text = crate::agents::tail(&run, max)?;

            Ok(format!(
                "{} ({}) — {}\n{}",
                run.name, run.state, run.title, text
            ))
        }
        "agent.kill" => {
            let id = args["id"].as_str().ok_or("agent.kill needs an id")?;

            if crate::agents::kill(gw, id)? {
                Ok(format!("sub-agent {id} is being stopped"))
            } else {
                Ok(format!("sub-agent {id} was not running"))
            }
        }
        "code.run" => {
            let command = args
                .get("command")
                .and_then(|v| v.as_str())
                .filter(|s| !s.trim().is_empty())
                .ok_or("missing command")?;
            let origin = sandbox::origin_of_tool(name, args_json);

            let out = sandbox::run(
                gw,
                sandbox::Request {
                    tool: name,
                    command,
                    profile: sandbox::Profile::Restricted,
                    cwd: args.get("cwd").and_then(|v| v.as_str()),
                    elevated: false,
                    permission,
                    timeout_secs: args["timeout"].as_u64(),
                    origin: origin.as_ref(),
                    background: false,
                    log: None,
                },
                None,
            )
            .await
            .map_err(|err| err.to_string())?;

            Ok(format!("exit {}\n{}", out.exit, out.combined()))
        }
        "grep" => grep::run(&args).await,
        "fs.write" => fs::write(&args),
        "fs.read" => {
            let conn = gw.conn.lock().map_err(|e| e.to_string())?;
            let out = fs::read::read(&conn, &gw.library_dir, &args);
            drop(conn);
            out
        }
        "doc.create" => {
            let sid = crate::tools::notepad::current_session();
            let (item, pages) = crate::library::doc::create(gw, &args, sid.as_deref())?;
            Ok(format!(
                "id={}\npath={}\nname={}\nkind={}\next={}\npages={}",
                item.id, item.path, item.name, item.kind, item.ext, pages
            ))
        }
        "skill.read" => {
            let name = args
                .get("name")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .ok_or("missing name")?;
            let conn = gw.conn.lock().map_err(|err| err.to_string())?;

            if let Some(file) = args
                .get("file")
                .and_then(|v| v.as_str())
                .filter(|f| !f.is_empty())
            {
                let content = crate::skills::store::read_file(&gw.skills_dir, name, file)?;
                return Ok(content);
            }

            let sk = crate::skills::store::get_skill(&conn, &gw.skills_dir, name)?;
            let _ = crate::skills::store::touch_skill(&conn, name);
            Ok(format!("{}\n{}", sk.description, sk.body))
        }
        "skill.search" => {
            let query = args
                .get("query")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            let conn = gw.conn.lock().map_err(|err| err.to_string())?;
            let found = if query.is_empty() {
                crate::skills::store::list_skills(&conn)?
            } else {
                crate::skills::store::search_skills(&conn, &query, 20)?
            };

            Ok(found
                .iter()
                .map(|s| format!("- {}: {}", s.name, s.description))
                .collect::<Vec<_>>()
                .join("\n"))
        }
        "skill.create" => {
            let name = args
                .get("name")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .ok_or("missing name")?;
            let description = args
                .get("description")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .ok_or("missing description")?;
            let body = args
                .get("body")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .ok_or("missing body")?;

            let conn = gw.conn.lock().map_err(|err| err.to_string())?;

            if crate::skills::store::search_skills(&conn, name, 5)?
                .iter()
                .any(|s| s.name == name)
            {
                return Err(format!(
                    "skill '{name}' already exists — read it first, then improve it instead"
                ));
            }

            let sk = crate::skills::store::create_skill(
                &conn,
                &gw.skills_dir,
                &crate::skills::schema::NewSkill {
                    name: name.into(),
                    description: description.into(),
                    body: body.into(),
                    source: Some("agent".into()),
                    origin: None,
                },
            )?;

            Ok(format!("saved skill '{}': {}", sk.name, sk.description))
        }
        "memory.save" => {
            let content = args
                .get("content")
                .and_then(|v| v.as_str())
                .filter(|s| !s.trim().is_empty())
                .ok_or("missing content")?;
            let kind = args
                .get("kind")
                .and_then(|v| v.as_str())
                .unwrap_or("fact")
                .to_string();
            let conn = gw.conn.lock().map_err(|err| err.to_string())?;
            let m = crate::memory::store::save(
                &conn,
                &crate::memory::schema::NewMemory {
                    content: content.into(),
                    kind: Some(kind),
                    importance: args.get("importance").and_then(|v| v.as_i64()),
                    session_id: None,
                },
            )?;
            Ok(format!("saved memory '{}': {}", m.id, m.content))
        }
        "memory.search" => {
            let query = args
                .get("query")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            let conn = gw.conn.lock().map_err(|err| err.to_string())?;
            let hits = crate::memory::store::recall(&conn, &query, 12)?;
            Ok(hits
                .iter()
                .map(|h| format!("[{}:{}] {}", h.source, h.ref_id, h.snippet))
                .collect::<Vec<_>>()
                .join("\n"))
        }
        "memory.read" => {
            let id = args
                .get("id")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .ok_or("missing id")?;
            let conn = gw.conn.lock().map_err(|err| err.to_string())?;
            let m = crate::memory::store::get(&conn, id)?;
            Ok(m.content)
        }
        "conversation.search" => {
            let query = args
                .get("query")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            let conn = gw.conn.lock().map_err(|err| err.to_string())?;
            let cur = crate::tools::notepad::current_session();
            let hits = crate::memory::store::recall_sessions(&conn, &query, cur.as_deref(), 5)?;

            if hits.is_empty() {
                return Ok("no earlier conversation matched that".into());
            }

            Ok(hits
                .iter()
                .map(|p| {
                    let body = p.snippets.join("\n  ");
                    format!("[{}] {}\n  {}", p.session_id, p.title, body)
                })
                .collect::<Vec<_>>()
                .join("\n"))
        }
        "conversation.read" => {
            let sid = args
                .get("session_id")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .ok_or("conversation.read needs a session_id")?;
            let after_seq = args.get("after_seq").and_then(|v| v.as_i64()).unwrap_or(0);
            let conn = gw.conn.lock().map_err(|err| err.to_string())?;
            let t = crate::memory::store::read_session(&conn, sid, after_seq, 20)?;

            if t.turns.is_empty() && !t.more {
                return Ok("that conversation has nothing readable in it".into());
            }

            let body = t
                .turns
                .iter()
                .map(|x| format!("{}: {}", x.who, x.text))
                .collect::<Vec<_>>()
                .join("\n\n");

            let more = if t.more {
                format!(
                    "\n\n(more remains — call again with after_seq {})",
                    t.next_seq
                )
            } else {
                String::new()
            };

            Ok(format!("[{}] {}\n\n{body}{more}", t.session_id, t.title))
        }
        "web.search" => web::search(&args).await,
        "web.read" => web::read(&args).await,
        "browser.open" => browser::open(&args).await,
        "browser.click" => browser::click(&args).await,
        "browser.type" => browser::type_text(&args).await,
        "browser.read" => browser::read(&args).await,
        "browser.scroll" => browser::scroll(&args).await,
        "browser.close" => browser::close(&args).await,
        "notepad.read" => notepad::read(&args),
        "notepad.append" => notepad::append(&args),
        "notepad.replace" => notepad::replace(&args),
        "notepad.clear" => notepad::clear(&args),
        "library.read" => {
            let conn = gw.conn.lock().map_err(|e| e.to_string())?;
            let out = library::read(&conn, &gw.library_dir, &args);
            drop(conn);
            out
        }
        "profile.list" | "profile.read" => {
            let conn = gw.conn.lock().map_err(|e| e.to_string())?;

            let out = if name == "profile.list" {
                profile::list(&conn, &args)
            } else {
                profile::read(&conn, &args)
            };

            drop(conn);

            out
        }
        _ if connector::META.iter().any(|t| t.name == name) => connector::exec(name, &args).await,
        _ if conn_oauth::META.iter().any(|t| t.name == name) => conn_oauth::exec(name, &args).await,
        _ => Err("unknown tool".into()),
    }
}
