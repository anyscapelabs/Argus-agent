use serde_json::Value;

use super::ToolMeta;

use crate::sessions::ext_install::data_dir;

pub const META: &[ToolMeta] = &[
    ToolMeta {
        name: "notepad.read",
        desc: "read the scratchpad: your private working notes for this session, or the shared global scratchpad",
        args: "{\"scope\":\"session\"}",
        mutating: false,
    },
    ToolMeta {
        name: "notepad.append",
        desc: "append a line to the scratchpad: running notes, leads, partial results",
        args: "{\"scope\":\"session\",\"text\":\"...\"}",
        mutating: false,
    },
    ToolMeta {
        name: "notepad.replace",
        desc: "rewrite the whole scratchpad with new text: use to condense or restructure notes",
        args: "{\"scope\":\"session\",\"text\":\"...\"}",
        mutating: false,
    },
    ToolMeta {
        name: "notepad.clear",
        desc: "erase the scratchpad entirely when its notes are spent",
        args: "{\"scope\":\"session\"}",
        mutating: false,
    },
];

tokio::task_local! {
    pub static SESSION_ID: Option<String>;
}

pub fn current_session() -> Option<String> {
    SESSION_ID.try_with(|v| v.clone()).unwrap_or(None)
}

const MAX_BYTES: u64 = 8192;
pub const INCLUDE_BYTES: usize = 1024;

fn dir() -> std::path::PathBuf {
    data_dir().join("notepad")
}

fn path_for(scope: &str, session: Option<&str>) -> Result<std::path::PathBuf, String> {
    match scope {
        "global" => Ok(dir().join("scratch.md")),
        "session" | "" => {
            let id = session
                .map(|s| {
                    s.chars()
                        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
                        .take(64)
                        .collect::<String>()
                })
                .filter(|s| !s.is_empty())
                .ok_or_else(|| "session scratchpad needs a session context".to_string())?;
            Ok(dir().join(format!("{id}.md")))
        }
        _ => Err("scope must be \"session\" or \"global\"".into()),
    }
}

fn scope_of(args: &Value) -> &str {
    args.get("scope")
        .and_then(|v| v.as_str())
        .unwrap_or("session")
}

fn text_of(args: &Value) -> Result<String, String> {
    args.get("text")
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
        .map(str::to_string)
        .ok_or_else(|| "notepad needs text".to_string())
}

fn check_cap(existing: u64, add: u64) -> Result<(), String> {
    if existing.saturating_add(add) > MAX_BYTES {
        return Err("notepad is full (8KB) — condense with notepad.replace or reset with notepad.clear first".into());
    }

    Ok(())
}

pub fn read(args: &Value) -> Result<String, String> {
    let path = path_for(scope_of(args), current_session().as_deref())?;
    match std::fs::read_to_string(&path) {
        Ok(s) if !s.trim().is_empty() => Ok(s),
        _ => Ok("(notepad empty)".into()),
    }
}

pub fn append(args: &Value) -> Result<String, String> {
    let path = path_for(scope_of(args), current_session().as_deref())?;
    let mut text = text_of(args)?;
    if !text.ends_with('\n') {
        text.push('\n');
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let existing = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    check_cap(existing, text.len() as u64)?;
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .and_then(|mut f| {
            use std::io::Write;
            f.write_all(text.as_bytes())
        })
        .map_err(|err| err.to_string())?;
    Ok("appended".into())
}

pub fn replace(args: &Value) -> Result<String, String> {
    let path = path_for(scope_of(args), current_session().as_deref())?;
    let text = text_of(args)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    check_cap(0, text.len() as u64)?;
    std::fs::write(&path, &text).map_err(|err| err.to_string())?;
    Ok("replaced".into())
}

pub fn clear(args: &Value) -> Result<String, String> {
    let path = path_for(scope_of(args), current_session().as_deref())?;
    match std::fs::remove_file(&path) {
        Ok(()) | Err(_) => Ok("notepad cleared".into()),
    }
}

pub fn prompt_include(session_id: &str) -> Option<String> {
    let path = path_for("session", Some(session_id)).ok()?;
    let text = std::fs::read_to_string(&path).ok()?;
    if text.trim().is_empty() {
        return None;
    }

    if text.len() <= INCLUDE_BYTES {
        return Some(format!("<working-notes>\n{text}\n</working-notes>"));
    }

    let head: String = text.chars().take(INCLUDE_BYTES).collect();
    Some(format!(
        "<working-notes>\n{head}\n…truncated, use notepad.read to see the rest.\n</working-notes>"
    ))
}
