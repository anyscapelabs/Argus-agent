use serde::Serialize;
use tauri::State;

use crate::Gateway;

pub mod usage;

/// `Local` answers here, `Prompt` expands to text for the model, `Client` lives
/// in the window.
#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "camelCase")]
pub enum Kind {
    Local,
    Prompt,
    Client,
}

#[derive(Serialize, Clone, Copy)]
#[serde(rename_all = "camelCase")]
pub struct Cmd {
    pub name: &'static str,
    pub desc: &'static str,
    pub kind: Kind,
    /// The placeholder shown after the name. `None` takes no argument.
    pub arg: Option<&'static str>,
    /// Prompt macros only. `{{arg}}` is replaced before the text is sent.
    pub tpl: Option<&'static str>,
}

pub const CMDS: &[Cmd] = &[
    Cmd {
        name: "usage",
        desc: "Requests, tokens, spend and hours worked.",
        kind: Kind::Local,
        arg: Some("today | week | month"),
        tpl: None,
    },
    Cmd {
        name: "compact",
        desc: "Summarise this conversation into a shorter context.",
        kind: Kind::Local,
        arg: None,
        tpl: None,
    },
    Cmd {
        name: "clear",
        desc: "Remove the files waiting to be sent with the next message.",
        kind: Kind::Client,
        arg: None,
        tpl: None,
    },
    Cmd {
        name: "help",
        desc: "List every command.",
        kind: Kind::Local,
        arg: None,
        tpl: None,
    },
    Cmd {
        name: "review",
        desc: "Read the changes and report what is wrong with them.",
        kind: Kind::Prompt,
        arg: Some("path"),
        tpl: Some(
            "Review the code at {{arg}}. Report what is wrong with it, \
                  ordered by how much it matters, and say plainly if you find \
                  nothing.",
        ),
    },
];

/// Exactly one is ever set: `text` speaks, `usage` draws a card, `model` sends
/// something instead of what the user typed.
#[derive(Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct SlashOut {
    pub text: Option<String>,
    pub model: Option<String>,
    pub usage: Option<usage::Usage>,
}

impl SlashOut {
    fn said(text: String) -> Self {
        Self {
            text: Some(text),
            ..Default::default()
        }
    }

    fn usage(u: usage::Usage) -> Self {
        Self {
            usage: Some(u),
            ..Default::default()
        }
    }
}

fn find(name: &str) -> Option<&'static Cmd> {
    CMDS.iter().find(|c| c.name == name)
}

/// No model, no tokens, so no turn is stored for it.
pub fn answered_locally(name: &str) -> bool {
    matches!(name, "usage")
}

/// Check before the row is written.
pub fn local_turn_text(name: &str, arg: &str) -> Result<String, String> {
    if !answered_locally(name) {
        return Err(format!("/{name} is not answered by the app itself."));
    }

    let arg = arg.trim();

    if name == "usage" {
        usage::window_of(arg)?;
    }

    Ok(match arg.is_empty() {
        true => format!("/{name}"),
        false => format!("/{name} {arg}"),
    })
}

#[tauri::command]
pub fn slash_list() -> Vec<Cmd> {
    CMDS.to_vec()
}

#[tauri::command]
pub async fn slash_run(
    gw: State<'_, Gateway>,
    session_id: Option<String>,
    name: String,
    arg: Option<String>,
) -> Result<SlashOut, String> {
    let Some(cmd) = find(&name) else {
        return Err(format!("no command /{name}. Try /help for the list."));
    };

    let arg = arg.unwrap_or_default().trim().to_string();

    // Answering here would claim an effect the window has already had.
    if cmd.kind == Kind::Client {
        return Err(format!("/{name} is handled by the app, not the backend."));
    }

    // An empty macro arg produces a prompt about nothing. Locals supply a default.
    if cmd.kind == Kind::Prompt && arg.is_empty() {
        return Err(format!(
            "/{} needs an argument: {}",
            name,
            cmd.arg.unwrap_or("")
        ));
    }

    match cmd.name {
        "usage" => {
            let conn = gw.conn.lock().map_err(|err| err.to_string())?;
            Ok(SlashOut::usage(usage::report(&conn, &arg)?))
        }
        "compact" => compact(&gw, session_id.as_deref()).await,
        "help" => Ok(SlashOut::said(help())),
        _ => Ok(SlashOut {
            model: Some(cmd.tpl.unwrap_or("").replace("{{arg}}", &arg)),
            ..Default::default()
        }),
    }
}

async fn compact(gw: &Gateway, session_id: Option<&str>) -> Result<SlashOut, String> {
    let Some(session_id) = session_id else {
        return Err("/compact needs a conversation to compact.".into());
    };

    let Some(rec) = crate::prompt::compressor::compact(gw, session_id).await? else {
        return Ok(SlashOut::said(
            "Nothing to compact — the conversation is still short.".into(),
        ));
    };

    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
    let status = crate::prompt::compressor::check(&conn, session_id, &gw.library_dir)?;

    Ok(SlashOut::said(format!(
        "Compacted through message {} with {}. The context is now about {} tokens.",
        rec.covers_to,
        rec.summary_model,
        usage::tokens(status.est_tok_in)
    )))
}

pub fn help() -> String {
    let mut out = String::from("**Commands**\n\n");

    for (i, c) in CMDS.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }

        let tail = match c.kind {
            Kind::Prompt => " — expands to a prompt",
            Kind::Client => " — handled by the app",
            Kind::Local => "",
        };

        match c.arg {
            Some(a) => out.push_str(&format!("/{} {} — {}{}\n", c.name, a, c.desc, tail)),
            None => out.push_str(&format!("/{} — {}{}\n", c.name, c.desc, tail)),
        }
    }

    out
}
