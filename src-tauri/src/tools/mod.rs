pub mod fs;
pub mod grep;
pub mod shell;
pub mod web;

use serde_json::Value;
use tauri::ipc::Channel;

use crate::gateway::schema::StreamEvent;

const MAX_OUT: usize = 6000;

pub struct ToolMeta {
    pub name: &'static str,
    pub desc: &'static str,
    pub args: &'static str,
    pub mutating: bool,
}

const TOOLS: &[ToolMeta] = &[
    ToolMeta {
        name: "terminal",
        desc: "run a shell command; output streams live to the user, 120s cap",
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
];

const WEB_TOOLS: &[ToolMeta] = &[
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

pub fn parse_actions(text: &str) -> Vec<Action> {
    let mut out = vec![];
    let mut rest = text;
    let mut off = 0usize;

    while let Some(start) = rest.find("<action") {
        let tail = &rest[start..];
        let end = match tail.find("</action>") {
            Some(e) => e,
            None => break,
        };

        let blk = &tail[..end];
        let tool = blk
            .split("tool=\"")
            .nth(1)
            .and_then(|s| s.split('"').next())
            .unwrap_or("")
            .to_string();

        let args = match blk.find('>') {
            Some(i) => blk[i + 1..].trim().to_string(),
            None => String::new(),
        };

        if !tool.is_empty() {
            out.push(Action {
                tool,
                args,
                start: off + start,
                end: off + start + end + 9,
            });
        }

        off += start + end + 9;
        rest = &tail[end + 9..];
    }

    out
}

const TOOL_CALL_CLOSE: &str = "</tool_call>";

pub fn normalize_actions(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;

    while let Some(start) = rest.find("<tool_call") {
        out.push_str(&rest[..start]);
        let tail = &rest[start..];

        let close = match tail.find(TOOL_CALL_CLOSE) {
            Some(e) => e,
            None => return out,
        };

        if let Some((tool, args)) = salvage_call(&tail[..close]) {
            out.push_str(&format!("<action tool=\"{tool}\">{args}</action>"));
        }

        rest = &tail[close + TOOL_CALL_CLOSE.len()..];
    }

    out.push_str(rest);
    out
}

fn salvage_call(inner: &str) -> Option<(String, String)> {
    let body = inner.strip_prefix("<tool_call")?.trim_start();
    let body = body.strip_prefix('>').unwrap_or(body).trim();

    if body.starts_with('{') {
        let v: Value = serde_json::from_str(body).ok()?;
        let tool = v.get("name")?.as_str()?.to_string();
        let args = v
            .get("arguments")
            .cloned()
            .unwrap_or_else(|| Value::Object(Default::default()));

        if !args.is_object() {
            return None;
        }

        return Some((tool, args.to_string()));
    }

    let mut lines = body.lines().map(str::trim).filter(|l| !l.is_empty());
    let tool = lines.next()?.to_string();

    if !tool
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
    {
        return None;
    }

    let keys = collect_spans(body, "<arg_key>", "</arg_key>");
    let vals = collect_spans(body, "<arg_value>", "</arg_value>");

    if keys.is_empty() {
        return None;
    }

    let mut args = serde_json::Map::new();
    for (k, v) in keys.into_iter().zip(vals) {
        args.insert(k, Value::String(v));
    }

    Some((tool, Value::Object(args).to_string()))
}

fn collect_spans(body: &str, open: &str, close: &str) -> Vec<String> {
    let mut out = vec![];
    let mut rest = body;

    while let Some(i) = rest.find(open) {
        let tail = &rest[i + open.len()..];
        let e = match tail.find(close) {
            Some(e) => e,
            None => break,
        };

        out.push(tail[..e].trim().to_string());
        rest = &tail[e + close.len()..];
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn salvages_native_key_value_tool_call() {
        let t = "<tool_callweb.search\n<arg_key>query</arg_key>\n<arg_value>\"Announcing Rust 1.98.0\" blog.rust-lang.org</arg_value>\n</tool_call>";
        let out = normalize_actions(t);
        assert!(
            out.contains(r#"<action tool="web.search">{"query":"#),
            "got: {out}"
        );
        let acts = parse_actions(&out);
        assert_eq!(acts.len(), 1);
        assert_eq!(acts[0].tool, "web.search");
        let args: Value = serde_json::from_str(&acts[0].args).expect("args json");
        assert_eq!(
            args["query"],
            "\"Announcing Rust 1.98.0\" blog.rust-lang.org"
        );
    }

    #[test]
    fn salvages_json_tool_call() {
        let t = "pre <tool_call{\"name\":\"web.read\",\"arguments\":{\"url\":\"https://x.y\"}}</tool_call> post";
        let acts = parse_actions(&normalize_actions(t));
        assert_eq!(acts.len(), 1);
        assert_eq!(acts[0].tool, "web.read");
        let args: Value = serde_json::from_str(&acts[0].args).expect("args json");
        assert_eq!(args["url"], "https://x.y");
        assert!(normalize_actions(t).starts_with("pre "));
        assert!(normalize_actions(t).ends_with(" post"));
    }

    #[test]
    fn drops_unsalvageable_tool_call() {
        assert_eq!(normalize_actions("<tool_call???' </tool_call>"), "");
        assert_eq!(normalize_actions("a <tool_callweb.search b"), "a ");
    }
}

pub async fn exec(
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
        .find(|t| t.name == name)
        .ok_or_else(|| format!("unknown tool {name}"))?;

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
            let (idx, chan) = match on_term {
                Some((c, i)) => (i, Some(c)),
                None => (0, None),
            };

            let (out, code) = shell::run_stream(&args, idx, chan).await?;
            Ok(format!("exit {code}\n{out}"))
        }
        "grep" => grep::run(&args).await,
        "fs.write" => fs::write(&args),
        "web.search" => web::search(&args).await,
        "web.read" => web::read(&args).await,
        _ => Err("unknown tool".into()),
    }
}

pub fn is_mutating(name: &str) -> bool {
    TOOLS
        .iter()
        .chain(WEB_TOOLS.iter())
        .find(|t| t.name == name)
        .map(|t| t.mutating)
        .unwrap_or(false)
}

pub fn section(web: bool) -> String {
    let mut s = String::from(
        "\n\n## Tools\n\
You work in steps. To run a tool, end your reply with an action block:\n\
<action tool=\"fs.write\">{\"path\":\"~/notes.txt\",\"content\":\"hello\"}</action>\n\
Several action blocks in one reply run in order. Results come back as the next \
message wrapped in <tool-result tool=\"...\" status=\"ok|err\">output</tool-result>. \
Then continue: act again or write the final answer with no action block.\n\
Never invent tool output, never claim a tool ran without an action block, never \
wrap an action block inside another tag. Never use <tool_call> or any other tool-call \
format — the action block is the only way to run a tool.\n\
If a tool result has status err, never run the same action again. Tell the user \
what failed in plain words and what would fix it.\n\
Terminal commands may need the user's approval; if one is denied, never retry it.\n\
Available tools:\n",
    );

    for t in TOOLS {
        s.push_str(&format!("- {} — {}. args: {}\n", t.name, t.desc, t.args));
    }

    if web {
        s.push_str("Web tools:\n");
        for t in WEB_TOOLS {
            s.push_str(&format!("- {} — {}. args: {}\n", t.name, t.desc, t.args));
        }
        s.push_str(
            "Cite what you used: after web.search or web.read, mention the source url in the reply.\n",
        );
    }

    s
}

fn expand(p: &str) -> String {
    let home = std::env::var("HOME").unwrap_or_default();

    if p == "~" {
        return home;
    }

    match p.strip_prefix("~/") {
        Some(rest) if !home.is_empty() => format!("{home}/{rest}"),
        _ => p.into(),
    }
}

pub fn clip(s: String) -> String {
    if s.chars().count() <= MAX_OUT {
        return s;
    }

    let cut: String = s.chars().take(MAX_OUT).collect();
    format!("{cut}\n...[truncated]")
}

pub fn clip_ends(s: String) -> String {
    let n = s.chars().count();

    if n <= MAX_OUT {
        return s;
    }

    let half = MAX_OUT / 2;
    let head: String = s.chars().take(half).collect();
    let tail: String = s.chars().skip(n - half).collect();

    format!("{head}\n...[truncated]...\n{tail}")
}
