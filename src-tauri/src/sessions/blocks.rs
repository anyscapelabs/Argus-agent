// Replacement order matters: `&` first, or later steps get escaped twice.

use std::sync::OnceLock;

use crate::gateway::schema::WireMsg;
use crate::gateway::Gateway;
use crate::tools;

use super::store;

pub fn esc_attr(s: &str) -> String {
    // A value can legally contain all of these; the tag cannot.
    s.replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('\n', "&#10;")
        .replace('\r', "&#13;")
}

pub fn terminal_block(idx: usize, cmd: &str, code: i64, out: &str, ms: u128) -> String {
    let status = if code == 0 { "ok" } else { "error" };
    let body = out.replace('&', "&amp;").replace('<', "&lt;");

    format!(
        "<terminal id=\"a{idx}\" command=\"{}\" status=\"{status}\" duration_ms=\"{ms}\">{}</terminal>",
        esc_attr(cmd),
        body.trim()
    )
}

pub fn exit_of(body: &str) -> i64 {
    body.strip_prefix("exit ")
        .and_then(|r| r.split_once('\n'))
        .and_then(|(c, _)| c.parse::<i64>().ok())
        .unwrap_or(-1)
}

pub fn browser_what(tool: &str, v: &serde_json::Value, masked: bool) -> String {
    let text = v.get("text").and_then(|t| t.as_str()).map(|s| {
        if masked {
            "····".into()
        } else {
            s.to_string()
        }
    });
    let what = v
        .get("url")
        .and_then(|u| u.as_str())
        .map(Into::into)
        .or(text)
        .unwrap_or_default();

    v.get("ref")
        .and_then(|r| r.as_u64())
        .map_or(format!("{tool} {what}"), |r| {
            format!("{tool} ref {r} {what}")
        })
}

pub fn browser_block(idx: usize, tool: &str, url: &str, what: &str) -> String {
    format!(
        "<browser-action id=\"a{idx}\" url=\"{}\" action=\"{}\">{}</browser-action>",
        esc_attr(url),
        esc_attr(tool),
        esc_attr(what)
    )
}

pub fn doc_field(body: &str, key: &str) -> String {
    body.lines()
        .find(|l| l.starts_with(key))
        .map(|l| l[key.len()..].trim().to_string())
        .unwrap_or_default()
}

pub fn doc_block(id: &str, title: &str, doctype: &str, pages: &str) -> String {
    format!(
        "<document id=\"{}\" title=\"{}\" doctype=\"{}\" pages=\"{}\" status=\"ready\" />",
        esc_attr(id),
        esc_attr(title),
        esc_attr(doctype),
        esc_attr(pages)
    )
}

pub fn body_url(body: &str) -> String {
    body.lines()
        .find(|l| l.starts_with("url "))
        .map(|l| l[4..].trim().to_string())
        .unwrap_or_default()
}

pub fn shot_marker(line: &str) -> Option<String> {
    let p = line
        .split("screenshot: ")
        .nth(1)?
        .split_whitespace()
        .next()?
        .trim();

    (p.ends_with(".png") && p.contains("/screenshots/shot-")).then(|| p.to_string())
}

pub fn attach_shots(msgs: &mut [WireMsg]) {
    let mut left = 2usize;

    for m in msgs.iter_mut().rev() {
        let paths: Vec<String> = m.content.lines().filter_map(shot_marker).collect();

        if paths.is_empty() || left == 0 {
            continue;
        }

        // Extend, not replace: a user image is no reason to drop a screenshot.
        m.images.extend(paths);
        left -= 1;
    }
}

// One predicate so the rule cannot drift between callers.
pub fn needs_text_blocks(style: tools::ToolCallStyle, degraded: bool, events_ok: bool) -> bool {
    style != tools::ToolCallStyle::Native || degraded || !events_ok
}

/// The goal is the task as first asked, not the word that resumed it.
pub fn save_resume(gw: &Gateway, session_id: &str, actions: &[(String, bool)]) {
    let Ok(conn) = gw.conn.lock() else {
        return;
    };
    let goal = store::first_user_msg(&conn, session_id).unwrap_or_default();
    super::resume::save(&conn, session_id, &goal, actions);
}

/// Each check matches a string Argus itself wrote, so a model cannot talk its
/// way into a lesson.
pub fn observe_exec(
    conn: &rusqlite::Connection,
    model_id: &str,
    exec: &tools::ToolExecution,
    thrashed: bool,
) {
    observe_signals(conn, model_id, exec, thrashed)
}

pub fn observe_signals(
    conn: &rusqlite::Connection,
    model_id: &str,
    exec: &tools::ToolExecution,
    thrashed: bool,
) {
    use crate::playbook::store::{record, Kind};

    let err = exec.error.as_deref().unwrap_or_default();

    if err.contains(EMPTY_ARGS_ERR) {
        let _ = record(conn, Kind::EmptyArgs, model_id, None, &exec.tool);
    }

    if thrashed {
        let _ = record(conn, Kind::Thrashing, model_id, None, &exec.tool);
    }

    let host = crate::sessions::ext_install::host_id();

    if err.contains(MISSING_CWD_ERR) {
        let _ = record(conn, Kind::SandboxNoCwd, &host, None, &exec.tool);
    }

    if err.contains(crate::tools::sandbox::DENIAL_NOTE) || denial_in_output(exec) {
        let _ = record(conn, Kind::SandboxDenied, &host, None, &exec.tool);
    }
}

/// A refusal is appended after the command's own output, so it is the tail.
/// Residual: a command could print the exact string, costing one host lesson.
pub fn denial_in_output(exec: &tools::ToolExecution) -> bool {
    const TAIL_MAX: usize = 200;

    let body = exec.result_body();
    let Some(at) = body.rfind(crate::tools::sandbox::DENIAL_NOTE) else {
        return false;
    };

    body.len() - at <= TAIL_MAX
}

/// Phrases this crate writes about itself. Matching on them is what makes a
/// signal unforgeable: the model cannot emit them into a place we read.
const EMPTY_ARGS_ERR: &str = "arrived with empty arguments";
const MISSING_CWD_ERR: &str = "project profile needs cwd";

/// Read at record time: a user-approved edit rewrites `exec.args` later.
pub fn exec_label(exec: &tools::ToolExecution, is_term: bool) -> String {
    if is_term {
        let v: serde_json::Value = serde_json::from_str(&exec.args).unwrap_or_default();
        let cmd = v["command"].as_str().unwrap_or_default().trim();

        if !cmd.is_empty() {
            return format!("terminal: {cmd}");
        }
    }

    let args: serde_json::Value = serde_json::from_str(&exec.args).unwrap_or_default();
    hint_of_args(&args, &exec.tool)
}

/// A tool plus the one argument that identifies it. Two different `fs.write`
/// calls must not read as the same action in a resume.
pub fn hint_of_args(args: &serde_json::Value, tool: &str) -> String {
    for k in ["command", "url", "query", "name", "path", "pattern", "id"] {
        if let Some(s) = args[k].as_str() {
            let s = s.trim();

            if !s.is_empty() {
                let head: String = s.chars().take(80).collect();
                return format!("{tool} {head}");
            }
        }
    }

    tool.to_string()
}

pub fn sanitize_tags(s: &str) -> String {
    let mut t = s.to_string();
    t = t
        .replace("<strong>", "<bold>")
        .replace("</strong>", "</bold>");
    t = t.replace("<b>", "<bold>").replace("</b>", "</bold>");
    t = t.replace("<em>", "<italic>").replace("</em>", "</italic>");
    t = t.replace("<i>", "<italic>").replace("</i>", "</italic>");
    t = t
        .replace("<u>", "<underline>")
        .replace("</u>", "</underline>");
    t = t
        .replace("<a ", "<link ")
        .replace("<a>", "<link>")
        .replace("</a>", "</link>");

    // Compiled once: this runs on every assistant message.
    static DROP_TAGS: OnceLock<regex::Regex> = OnceLock::new();
    static BR: OnceLock<regex::Regex> = OnceLock::new();

    // Loud fail: a silent skip leaves the tags in the transcript with no signal.
    let drop_tags = DROP_TAGS.get_or_init(|| {
        regex::Regex::new(r"(?i)</?(p|div|span|command|output|think)[^>]*>")
            .expect("drop-tag pattern")
    });
    t = drop_tags.replace_all(&t, "").into_owned();

    let br = BR.get_or_init(|| regex::Regex::new(r"(?i)<br\s*/?>").expect("br pattern"));
    t = br.replace_all(&t, "\n").into_owned();

    t
}
