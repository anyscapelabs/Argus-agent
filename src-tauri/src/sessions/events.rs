use serde_json::Value;

use super::schema::NewEvent;
use crate::tools::ToolExecution;

// The structured twin of what used to be a markup block in message text.
// Both are built from the same ToolExecution, so the card, the history, and
// the audit can never disagree about what ran.
const OUTPUT_CAP: usize = 65536;

pub fn kind_of(tool: &str) -> &'static str {
    if tool == "terminal" || tool == "bash.run" {
        "terminal"
    } else if tool.starts_with("browser.") {
        "browser"
    } else if tool == "doc.create" {
        "document"
    } else if tool == "code.run" {
        "sandbox"
    } else {
        "action"
    }
}

fn clip_output(s: &str) -> String {
    if s.len() <= OUTPUT_CAP {
        return s.to_string();
    }

    let cut: String = s.chars().take(OUTPUT_CAP).collect();
    format!("{cut}\n...[truncated]")
}

fn arg(args: &Value, key: &str) -> String {
    args.get(key)
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string()
}

fn num_arg(args: &Value, key: &str) -> String {
    args.get(key)
        .and_then(|v| v.as_u64())
        .map(|n| n.to_string())
        .unwrap_or_default()
}

fn hint_of(args: &Value) -> String {
    for k in ["command", "url", "query", "name", "path", "pattern"] {
        let v = arg(args, k);

        if !v.is_empty() {
            return v.chars().take(60).collect();
        }
    }

    args.get("ref")
        .and_then(|v| v.as_u64())
        .map(|r| r.to_string())
        .unwrap_or_default()
}

fn doc_field(body: &str, key: &str) -> String {
    body.lines()
        .find(|l| l.starts_with(key))
        .map(|l| l[key.len()..].trim().to_string())
        .unwrap_or_default()
}

// Display text computed once at write time, mirroring the card labels the UI
// used to derive by re-parsing markup. One derivation, not one per reader.
fn label_for(kind: &str, tool: &str, args: &Value, output: &str) -> (String, String) {
    match kind {
        "terminal" => {
            let cmd = arg(args, "command");
            let cmd = if cmd.is_empty() { "shell".into() } else { cmd };
            let title = arg(args, "label");

            if title.is_empty() {
                (format!("$ {cmd}"), String::new())
            } else {
                (title, format!("$ {cmd}"))
            }
        }
        "browser" => {
            let verb = tool.split('.').next_back().unwrap_or("");
            let label = match verb {
                "open" => format!("Opened {}", arg(args, "url")),
                "click" => format!("Clicked {}", num_arg(args, "ref")),
                "type" => format!("Typed {}", num_arg(args, "ref")),
                "read" => {
                    let what = arg(args, "text");
                    if what.is_empty() {
                        "Read page".into()
                    } else {
                        format!("Read {what}")
                    }
                }
                "scroll" => {
                    let dir = arg(args, "direction");
                    format!("Scrolled {}", if dir.is_empty() { "down" } else { &dir })
                }
                "close" => "Closed tab".into(),
                _ => format!("Browsed {}", arg(args, "url")),
            };

            (label, String::new())
        }
        "document" => {
            let title = doc_field(output, "title=");
            let title = if title.is_empty() {
                doc_field(output, "name=")
            } else {
                title
            };

            (
                format!(
                    "Created document {}",
                    if title.is_empty() {
                        "document".into()
                    } else {
                        title
                    }
                ),
                String::new(),
            )
        }
        "sandbox" => {
            let cmd = arg(args, "command");
            (
                format!(
                    "$ {}",
                    if cmd.is_empty() {
                        "command".into()
                    } else {
                        cmd
                    }
                ),
                "sandboxed · restricted".into(),
            )
        }
        _ => {
            if tool == "gmail.send" || tool == "outlook.send" {
                return (format!("Email to {}", arg(args, "to")), String::new());
            }

            let hint = hint_of(args);

            (
                format!(
                    "Ran {tool}{}",
                    if hint.is_empty() {
                        String::new()
                    } else {
                        format!(" {hint}")
                    }
                ),
                String::new(),
            )
        }
    }
}

pub fn from_execution(exec: &ToolExecution, message_id: &str, session_id: &str) -> NewEvent {
    let kind = kind_of(&exec.tool);
    let args: Value = serde_json::from_str(&exec.args).unwrap_or(Value::Null);
    let (label, detail) = label_for(kind, &exec.tool, &args, exec.result_body());

    NewEvent {
        message_id: message_id.into(),
        session_id: session_id.into(),
        kind: kind.into(),
        tool: exec.tool.clone(),
        args_json: exec.args.clone(),
        status: exec.status.as_str().into(),
        elapsed_ms: exec.elapsed_ms.min(i64::MAX as u128) as i64,
        code: exit_code(exec),
        output: clip_output(exec.result_body()),
        label,
        detail,
    }
}

// The card shows `exit N`, and the UI must not re-parse output strings to get
// it. Same shape as the chat loop's exit_of: a leading `exit N` line wins,
// success without one is 0, anything else failed.
fn exit_code(exec: &ToolExecution) -> i64 {
    if exec.status == crate::tools::ToolStatus::Cancelled && exec.result_body().contains("denied") {
        return -2;
    }

    if let Some(rest) = exec.result_body().strip_prefix("exit ") {
        if let Some((code, _)) = rest.split_once('\n') {
            if let Ok(n) = code.trim().parse::<i64>() {
                return n;
            }
        }
    }

    if exec.status == crate::tools::ToolStatus::Succeeded {
        0
    } else {
        -1
    }
}
