pub mod browser;
pub mod conn_oauth;
pub mod connector;
pub mod dispatch;
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
pub use dispatch::{exec, ExecIn};
pub use parse::*;
pub use specs::*;
pub mod web;

use serde_json::Value;

use crate::gateway::schema::ToolCall;

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
        desc: "Execute commands on the user's computer. Use for: inspecting the system and files; creating or modifying files; running programs; builds and tests; Git; package managers; system administration. Use user privilege by default. Use admin privilege only when root access is required. Admin authentication is handled by the operating system. Never ask for or handle the user's sudo password. Set profile \"project\" to confine the command to this project's directory and its dependency caches, or \"restricted\" for code you do not trust. Commands are cut off after 120 seconds unless you set timeout, in seconds, up to 1800 — nothing picks that number for you, so judge it from the command you are about to run and raise it for a clone, a build or an install. Raise it before the run, not after: a command stopped by the timeout restarts from the beginning. Set background true for anything that outlives a few minutes — a long build, a big download, a migration. A backgrounded call returns a job id at once instead of waiting; check on it with job.list and read what it printed with job.read. Do not background a command you need the answer from before you can continue.",
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

// A text-channel call with `{}` is only breakage when the tool takes
// arguments. Several tools document `{}` as their whole invocation —
// browser.read, browser.close — so the empty-args guard asks the catalog
// instead of rejecting them all.
pub(crate) fn takes_no_args(name: &str) -> bool {
    TOOLS
        .iter()
        .chain(WEB_TOOLS.iter())
        .chain(browser::META.iter())
        .chain(notepad::META.iter())
        .chain(profile::META.iter())
        .chain(fs::read::META.iter())
        .chain(library::META.iter())
        .chain(connector::META.iter())
        .chain(conn_oauth::META.iter())
        .filter(|t| t.name == name)
        .any(|t| {
            serde_json::from_str::<serde_json::Map<String, Value>>(t.args)
                .is_ok_and(|m| m.is_empty())
        })
}

// One redaction for every secret shape. The browser guard knew token patterns
// (`sk-`, `ghp_`, …) and the connector log knew parameter names (`?key=`,
// `client_secret=`); a credential matching only one list passed the other.
// Both passes run here so no caller can pick the wrong half.
pub fn redact_secrets(s: &str) -> String {
    let patterned = match browser::guard::secret_re() {
        Some(re) => re.replace_all(s, "[redacted]").into_owned(),
        None => s.to_string(),
    };

    let mut out = patterned;

    for mark in ["?key=", "&key=", "?token=", "&token=", "client_secret="] {
        let mut from = 0;

        while let Some(i) = out[from..].find(mark) {
            let start = from + i + mark.len();
            let end = out[start..]
                .find(['&', ' ', '"', '\''])
                .map(|e| start + e)
                .unwrap_or(out.len());

            out.replace_range(start..end, "..redacted..");
            from = start + 12;
        }
    }

    out
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
