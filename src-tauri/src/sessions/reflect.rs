// Post-turn housekeeping: the reflection pass, and the session title.
//
// Both run after the agent loop has already returned, so both must tolerate
// a gateway that has gone away. A failed check is recorded, never raised.

use rusqlite::{params, OptionalExtension};

use crate::gateway::router;
use crate::gateway::schema::{ChatReq, WireMsg};
use crate::gateway::Gateway;
use crate::prompt::compressor;
use crate::prompt::config::truncate_chars;

const MAX_REFLECT_NUDGES: usize = 1;
const TITLE_SYS: &str = "Give a title of at most 5 words for this conversation. Reply with the title only, no quotes, no punctuation at the end.";

pub fn fakes_output(text: &str) -> bool {
    text.contains("<browser-action") || text.contains("<terminal")
}

pub fn has_faux_sandbox(text: &str) -> bool {
    text.contains("<sandbox")
}

pub fn should_reflect(reflect_on: bool, acts_run: usize, reflect_nudges: usize) -> bool {
    reflect_on && acts_run > 0 && reflect_nudges < MAX_REFLECT_NUDGES
}

pub fn parse_reflection_verdict(text: &str) -> Option<String> {
    let t = text.trim();
    let upper = t.to_ascii_uppercase();

    if upper == "PASS"
        || upper.starts_with("PASS ")
        || upper.starts_with("PASS\n")
        || upper.starts_with("PASS.")
        || upper.starts_with("PASS:")
    {
        return None;
    }

    Some(t.chars().take(2000).collect())
}

pub fn check_block(pass: bool) -> String {
    if pass {
        "<check status=\"pass\"/>".into()
    } else {
        "<check status=\"retry\"/>".into()
    }
}

pub async fn run_reflection_check(
    gw: &Gateway,
    session_id: &str,
    answer: &str,
) -> Result<Option<String>, String> {
    let (goal, model) = {
        let conn = gw.conn.lock().map_err(|err| err.to_string())?;
        let goal: Option<String> = conn
            .query_row(
                "SELECT content FROM messages WHERE session_id = ?1 AND role = 'user' \
                 AND active = 1 AND content NOT LIKE '<tool-result%' \
                 ORDER BY seq LIMIT 1",
                params![session_id],
                |r| r.get(0),
            )
            .optional()
            .map_err(|err| err.to_string())?;
        let Some(goal) = goal else {
            return Ok(None);
        };
        let model = crate::prompt::compressor::utility_model(&conn)?;
        (goal, model)
    };

    let instruction = format!(
        "You are Argus's answer checker. Does the reply below actually satisfy the goal \
         stated in the first message? Did any tool call actually fail without the reply \
         acknowledging it? Does the reply promise future work or ask the user to wait \
         instead of delivering the result? Reply with exactly PASS if yes to the first, \
         no to the second and third, otherwise reply with one short corrective instruction.\n\
         \n\
         GOAL:\n\
         {goal}\n\
         \n\
         REPLY:\n\
         {answer}"
    );

    let request = ChatReq {
        model,
        msgs: vec![WireMsg {
            role: "user".into(),
            content: instruction,
            ..Default::default()
        }],
        prefix_hash: None,
        tools: vec![],
    };

    let response = crate::gateway::router::run_opts(gw, &request, 1).await?;

    Ok(parse_reflection_verdict(&response.content))
}

pub async fn generate_title(gw: &Gateway, session_id: &str, content: &str) -> Result<(), String> {
    let (util, selected) = {
        let conn = gw.conn.lock().map_err(|err| err.to_string())?;
        let selected: Option<String> = conn
            .query_row(
                "SELECT model_id FROM sessions WHERE id = ?1",
                params![session_id],
                |r| r.get(0),
            )
            .map_err(|err| err.to_string())?;
        (compressor::utility_model(&conn), selected)
    };

    let msgs = vec![
        WireMsg {
            role: "system".into(),
            content: TITLE_SYS.into(),
            ..Default::default()
        },
        WireMsg {
            role: "user".into(),
            content: truncate_chars(content, 500),
            ..Default::default()
        },
    ];

    let raw = match util {
        Ok(u) => {
            let req = ChatReq {
                model: u,
                msgs: msgs.clone(),
                prefix_hash: None,
                tools: vec![],
            };
            match router::run_opts(gw, &req, 2).await {
                Ok(resp) => resp.content,
                Err(_) => fallback_title(gw, selected, &msgs).await?,
            }
        }
        Err(_) => fallback_title(gw, selected, &msgs).await?,
    };

    let title = clean_title(&raw).ok_or("title model returned nothing usable")?;

    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
    conn.execute(
        "UPDATE sessions SET title = ?2 WHERE id = ?1",
        params![session_id, title],
    )
    .map_err(|err| err.to_string())?;

    Ok(())
}

async fn fallback_title(
    gw: &Gateway,
    selected: Option<String>,
    msgs: &[WireMsg],
) -> Result<String, String> {
    let model = selected
        .filter(|m| !m.is_empty())
        .ok_or("no fallback model for title")?;

    let req = ChatReq {
        model,
        msgs: msgs.to_vec(),
        prefix_hash: None,
        tools: vec![],
    };

    Ok(router::run_opts(gw, &req, 2).await?.content)
}

pub fn clean_title(raw: &str) -> Option<String> {
    let line = raw.lines().next().unwrap_or("").trim();
    let t = line.trim_matches('"').trim_matches('\'').trim();

    if t.is_empty() || t.contains('<') || t.contains('>') {
        return None;
    }

    Some(truncate_chars(t, 60))
}
