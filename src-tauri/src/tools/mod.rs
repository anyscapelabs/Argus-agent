pub mod fs;
pub mod shell;

use serde_json::Value;

const MAX_OUT: usize = 6000;

pub struct ToolMeta {
  pub name: &'static str,
  pub desc: &'static str,
  pub args: &'static str,
  pub mutating: bool,
}

const TOOLS: &[ToolMeta] = &[
  ToolMeta { name: "fs.read", desc: "read a text file", args: "{\"path\":\"~/file.txt\"}", mutating: false },
  ToolMeta { name: "fs.list", desc: "list a directory", args: "{\"path\":\".\"}", mutating: false },
  ToolMeta { name: "fs.write", desc: "create or overwrite a text file", args: "{\"path\":\"...\",\"content\":\"...\"}", mutating: true },
  ToolMeta { name: "shell.run", desc: "run a shell command, 30s cap", args: "{\"command\":\"...\",\"cwd\":\".\"}", mutating: true },
];

pub struct Action {
  pub tool: String,
  pub args: String,
}

// Pull <action tool="...">args</action> blocks out of a reply.
pub fn parse_actions(text: &str) -> Vec<Action> {
  let mut out = vec![];
  let mut rest = text;
  while let Some(start) = rest.find("<action") {
    let tail = &rest[start..];
    let end = match tail.find("</action>") {
      Some(e) => e,
      None => break, // unclosed tag: not a call, drop it
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
      out.push(Action { tool, args });
    }
    rest = &tail[end + 9..];
  }
  out
}

pub async fn exec(name: &str, args_json: &str, permission: &str) -> Result<String, String> {
  let meta = TOOLS
    .iter()
    .find(|t| t.name == name)
    .ok_or_else(|| format!("unknown tool {name}"))?;
  if meta.mutating && permission == "ask" {
    return Err("blocked: this session asks before acting; switch its permission to never to allow writes".into());
  }
  let args: Value = serde_json::from_str(args_json.trim())
    .map_err(|_| "action body is not valid JSON")?;
  match name {
    "fs.read" => fs::read(&args),
    "fs.list" => fs::list(&args),
    "fs.write" => fs::write(&args),
    "shell.run" => shell::run(&args).await,
    _ => Err("unknown tool".into()),
  }
}

// Prompt section: the XML-only tool dialect, byte-stable across turns.
pub fn section() -> String {
  let mut s = String::from(
    "\n\n## Tools\n\
You work in steps. To run a tool, end your reply with an action block:\n\
<action tool=\"fs.read\">{\"path\":\"~/notes.txt\"}</action>\n\
Several action blocks in one reply run in order. Results come back as the next \
message wrapped in <tool-result tool=\"...\" status=\"ok|err\">output</tool-result>. \
Then continue: act again or write the final answer with no action block.\n\
Never invent tool output, never claim a tool ran without an action block, never \
wrap an action block inside another tag.\n\
Available tools:\n",
  );
  for t in TOOLS {
    s.push_str(&format!("- {} — {}. args: {}\n", t.name, t.desc, t.args));
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
