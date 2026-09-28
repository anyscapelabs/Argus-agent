pub mod browser;
pub mod conn_oauth;
pub mod connector;
pub mod fs;
pub mod grep;
pub mod library;
pub mod notepad;
pub mod parse;
pub mod profile;
pub mod recover;
pub mod sandbox;
pub mod shell;
pub mod specs;

// The catalog and the prompt text behind it, re-exported so callers keep
// reaching them at `tools::`.
pub use parse::*;
pub use specs::*;
pub mod web;

use serde_json::Value;
use tauri::ipc::Channel;

use crate::gateway::schema::{StreamEvent, ToolCall};

pub const MAX_OUT: usize = 6000;

pub struct ToolMeta {
    pub name: &'static str,
    pub desc: &'static str,
    pub args: &'static str,
    pub mutating: bool,
}

pub(crate) const TOOLS: &[ToolMeta] = &[
    ToolMeta {
        name: "terminal",
        desc: "Execute commands on the user's computer. Use for: inspecting the system and files; creating or modifying files; running programs; builds and tests; Git; package managers; system administration. Use user privilege by default. Use admin privilege only when root access is required. Admin authentication is handled by the operating system. Never ask for or handle the user's sudo password. Set profile \"project\" to confine the command to this project's directory and its dependency caches, or \"restricted\" for code you do not trust. Set background true for anything that outlives a few minutes — a long build, a big download, a migration. A backgrounded call returns a job id at once instead of waiting; check on it with job.list and read what it printed with job.read. Do not background a command you need the answer from before you can continue.",
        args: "{\"command\":\"...\",\"cwd\":\".\",\"label\":\"...\",\"privilege\":\"user\",\"profile\":\"host\",\"background\":false,\"timeout\":120}",
        mutating: true,
    },
    ToolMeta {
        name: "job.list",
        desc: "List background jobs, newest first, with their state (running, done, failed, killed, interrupted) and exit code. Filter to one session with session_id. Call this instead of sleeping or re-running a command to find out how it went.",
        args: "{\"session_id\":\"...\",\"limit\":20}",
        mutating: false,
    },
    ToolMeta {
        name: "job.read",
        desc: "Read the tail of a background job's output. Returns the last part of what it printed, oldest-first within the tail. Read it before deciding whether the work succeeded — the exit code alone rarely says. Output is trimmed from the front when it is long.",
        args: "{\"id\":\"...\",\"max_chars\":8000}",
        mutating: false,
    },
    ToolMeta {
        name: "job.kill",
        desc: "Stop a running background job. Use when it is clearly going the wrong way and the user did not ask for it to finish.",
        args: "{\"id\":\"...\"}",
        mutating: true,
    },
    ToolMeta {
        name: "agent.spawn",
        desc: "Start a sub-agent on one self-contained piece of a larger task and get a card back immediately. The prompt must stand alone — the sub-agent cannot see this conversation. Use it for work that splits into independent parts: researching N separate things, inspecting N separate files, auditing N separate call sites. Start every piece before waiting on any of them. At most four run at a time, and a sub-agent cannot start further sub-agents. When the last one finishes you are called back automatically with every result, so end your turn after starting them rather than sleeping or polling.",
        args: "{\"name\":\"...\",\"title\":\"...\",\"prompt\":\"...\",\"wake\":true}",
        mutating: true,
    },
    ToolMeta {
        name: "agent.list",
        desc: "List the sub-agents this conversation started, with their state (running, done, failed, interrupted) and their answer. Call this instead of waiting or re-asking.",
        args: "{}",
        mutating: false,
    },
    ToolMeta {
        name: "agent.read",
        desc: "Read one sub-agent's full answer. The summary you were given is trimmed; read the whole thing when the answer is load-bearing and the tail left a question open.",
        args: "{\"id\":\"...\",\"max_chars\":8000}",
        mutating: false,
    },
    ToolMeta {
        name: "agent.kill",
        desc: "Stop a running sub-agent. Use when it is clearly going the wrong way and the user did not ask it to finish.",
        args: "{\"id\":\"...\"}",
        mutating: true,
    },
    ToolMeta {
        name: "code.run",
        desc: "Run code from untrusted origins — anything fetched from the web, pasted scripts of unknown provenance, or a freshly cloned repo. Use for: building, testing, or inspecting untrusted code. It runs with no network access and can write only inside its own scratch directory. Never use it for your own files and projects; that is what terminal is for.",
        args: "{\"command\":\"...\",\"cwd\":\".\"}",
        mutating: true,
    },
    ToolMeta {
        name: "bash.run",
        desc: "legacy alias of terminal",
        args: "{\"command\":\"...\",\"cwd\":\".\"}",
        mutating: true,
    },
    ToolMeta {
        name: "grep",
        desc: "search file contents recursively",
        args: "{\"pattern\":\"...\",\"path\":\".\",\"ignore_case\":false}",
        mutating: false,
    },
    ToolMeta {
        name: "fs.write",
        desc: "create or overwrite a text file",
        args: "{\"path\":\"...\",\"content\":\"...\"}",
        mutating: true,
    },
    ToolMeta {
        name: "doc.create",
        desc: "create a document in the library: docx, pdf, pptx, xlsx, csv, md or txt",
        args: "{\"name\":\"...\",\"kind\":\"docx\",\"title\":\"...\",\"content\":\"...\"}",
        mutating: true,
    },
    ToolMeta {
        name: "skill.read",
        desc: "read a skill's SKILL.md by name; pass file (e.g. reference/api.md) for a reference doc listed in the skill's body",
        args: "{\"name\":\"...\",\"file\":\"\"}",
        mutating: false,
    },
    ToolMeta {
        name: "skill.search",
        desc: "search skills by keyword; empty query lists the full index",
        args: "{\"query\":\"...\"}",
        mutating: false,
    },
    ToolMeta {
        name: "skill.create",
        desc: "save a reusable skill: kebab-case name, one-line description, body with When to use, Steps, Pitfalls sections",
        args: "{\"name\":\"...\",\"description\":\"...\",\"body\":\"...\"}",
        mutating: true,
    },
    ToolMeta {
        name: "memory.save",
        desc: "save a durable memory: fact, preference, project, person or decision",
        args: "{\"content\":\"...\",\"kind\":\"fact\"}",
        mutating: true,
    },
    ToolMeta {
        name: "memory.search",
        desc: "search memories plus past messages, summaries and indexed files by keyword",
        args: "{\"query\":\"...\"}",
        mutating: false,
    },
    ToolMeta {
        name: "memory.read",
        desc: "read one memory by id",
        args: "{\"id\":\"...\"}",
        mutating: false,
    },
    ToolMeta {
        name: "conversation.search",
        desc: "Search your past conversations with the user — other sessions, not the current one. \
Returns the sessions that discussed this, with excerpts. Use it whenever the user refers to \
something from an earlier conversation (\"the Netflix case\", \"what did we decide about X\", \
\"that bug we fixed\"), or when the task needs context you were not given. Search with concrete \
keywords, not the whole sentence. To read what was actually said, follow up with \
conversation.read on the session id.",
        args: "{\"query\":\"...\"}",
        mutating: false,
    },
    ToolMeta {
        name: "conversation.read",
        desc: "Read what was actually said in a past conversation, as the user and you wrote it — \
not a summary. Pass the session id from conversation.search. Long chats are paged: if the result \
says more remains, call again with the returned seq to continue.",
        args: "{\"session_id\":\"...\",\"after_seq\":0}",
        mutating: false,
    },
];

pub(crate) const WEB_TOOLS: &[ToolMeta] = &[
    ToolMeta {
        name: "web.search",
        desc: "search the web, returns numbered results with title, url and snippet",
        args: "{\"query\":\"...\"}",
        mutating: false,
    },
    ToolMeta {
        name: "web.read",
        desc: "fetch a web page as plain text",
        args: "{\"url\":\"https://...\"}",
        mutating: false,
    },
];

pub struct Action {
    pub tool: String,
    pub args: String,
    pub start: usize,
    pub end: usize,
}

// How a model speaks tools. One mechanism for all models; only this differs.
// Native models call through the API and any text syntax is discarded.
// GlmXml models were fine-tuned on an XML template that contradicts the API
// instruction, so they emit both: the native call runs, the text is decoded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolCallStyle {
    Native,
    GlmXml,
}

impl ToolCallStyle {
    pub fn as_str(self) -> &'static str {
        match self {
            ToolCallStyle::Native => "native",
            ToolCallStyle::GlmXml => "glm-xml",
        }
    }

    pub fn style_for_family(family: Option<&str>) -> ToolCallStyle {
        let is_glm = family.unwrap_or_default().to_lowercase().contains("glm");

        if is_glm {
            ToolCallStyle::GlmXml
        } else {
            ToolCallStyle::Native
        }
    }

    pub fn from_caps(caps: Option<&str>) -> ToolCallStyle {
        let Some(c) = caps else {
            return ToolCallStyle::Native;
        };
        let Ok(v) = serde_json::from_str::<Value>(c) else {
            return ToolCallStyle::Native;
        };
        let is_xml = v
            .get("tool_call_style")
            .and_then(|s| s.as_str())
            .is_some_and(|s| s == "glm-xml");

        if is_xml {
            ToolCallStyle::GlmXml
        } else {
            ToolCallStyle::Native
        }
    }

    pub fn caps_with_style(caps: Option<&str>, style: ToolCallStyle) -> String {
        let mut obj = caps
            .and_then(|c| serde_json::from_str::<serde_json::Map<String, Value>>(c).ok())
            .unwrap_or_default();
        obj.insert(
            "tool_call_style".to_string(),
            Value::String(style.as_str().into()),
        );
        Value::Object(obj).to_string()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ToolStatus {
    Created,
    Executing,
    Succeeded,
    Failed,
    Cancelled,
}

impl ToolStatus {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            ToolStatus::Succeeded | ToolStatus::Failed | ToolStatus::Cancelled
        )
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            ToolStatus::Created => "created",
            ToolStatus::Executing => "executing",
            ToolStatus::Succeeded => "succeeded",
            ToolStatus::Failed => "failed",
            ToolStatus::Cancelled => "cancelled",
        }
    }
}

#[derive(Clone, Debug)]
pub struct ToolExecution {
    pub id: String,
    pub tool: String,
    pub args: String,
    pub status: ToolStatus,
    pub result: Option<String>,
    pub error: Option<String>,
    pub start: Option<usize>,
    pub end: Option<usize>,
    pub tool_call_id: Option<String>,
    pub elapsed_ms: u128,
}

impl ToolExecution {
    pub fn new(
        id: String,
        tool: String,
        args: String,
        start: Option<usize>,
        end: Option<usize>,
        tool_call_id: Option<String>,
    ) -> Self {
        Self {
            id,
            tool,
            args,
            status: ToolStatus::Created,
            result: None,
            error: None,
            start,
            end,
            tool_call_id,
            elapsed_ms: 0,
        }
    }

    pub fn from_native(call: &ToolCall, idx: usize) -> Self {
        if call.name.trim().is_empty() {
            let mut e = Self::new(
                if call.id.is_empty() {
                    format!("a{idx}")
                } else {
                    call.id.clone()
                },
                "(unknown)".into(),
                call.args.clone(),
                None,
                None,
                Some(call.id.clone()),
            );
            e.fail("model returned a tool call with no name — ignored".into());
            return e;
        }

        let args = if call.args.trim().is_empty() {
            "{}".to_string()
        } else {
            call.args.clone()
        };

        Self::new(
            call.id.clone(),
            call.name.clone(),
            args,
            None,
            None,
            Some(call.id.clone()),
        )
    }

    pub fn from_action(a: &Action, idx: usize) -> Self {
        Self::new(
            format!("a{idx}"),
            a.tool.clone(),
            a.args.clone(),
            Some(a.start),
            Some(a.end),
            None,
        )
    }

    pub fn begin(&mut self) {
        if self.status == ToolStatus::Created {
            self.status = ToolStatus::Executing;
        }
    }

    pub fn succeed(&mut self, result: String) {
        self.status = ToolStatus::Succeeded;
        self.result = Some(result);
        self.error = None;
    }

    pub fn fail(&mut self, err: String) {
        self.status = ToolStatus::Failed;
        self.error = Some(err);
        self.result = None;
    }

    pub fn cancel(&mut self, reason: String) {
        self.status = ToolStatus::Cancelled;
        self.error = Some(reason);
        self.result = None;
    }

    pub fn is_terminal_tool(&self) -> bool {
        self.tool == "terminal" || self.tool == "bash.run"
    }

    pub fn is_browser_tool(&self) -> bool {
        self.tool.starts_with("browser.")
    }

    fn body_inner(&self) -> &str {
        if let Some(r) = self.result.as_deref() {
            return r;
        }
        self.error.as_deref().unwrap_or("")
    }

    pub fn result_status(&self) -> &'static str {
        match self.status {
            ToolStatus::Succeeded => "ok",
            _ => "err",
        }
    }

    pub fn result_body(&self) -> &str {
        self.body_inner()
    }

    pub fn to_tool_result(&self, max_chars: usize) -> String {
        let body = self.body_inner();
        let clipped = if body.chars().count() <= max_chars {
            body.to_string()
        } else {
            let cut: String = body.chars().take(max_chars).collect();
            format!("{cut}…")
        };
        format!(
            "<tool-result tool=\"{}\" status=\"{}\">{}</tool-result>",
            self.tool,
            self.result_status(),
            clipped
        )
    }
}

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
