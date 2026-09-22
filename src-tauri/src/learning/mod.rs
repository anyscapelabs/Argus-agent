pub mod schema;
pub mod store;

use serde_json::Value;
use tauri::State;

use crate::gateway::schema::{ChatReq, WireMsg};
use crate::gateway::Gateway;

const MAX_PREFS_IN_PROMPT: usize = 12;
const MAX_EXAMPLES_IN_PROMPT: usize = 2;
const MIN_PROMPT_CONFIDENCE: f64 = 0.62;
const MIN_LEARN_CONFIDENCE: f64 = 0.72;

pub fn migrate(conn: &rusqlite::Connection) -> Result<(), String> {
    store::migrate(conn)
}

pub fn prompt_context(conn: &rusqlite::Connection) -> Result<String, String> {
    let preferences = store::list_preferences(conn, Some("global"))?
        .into_iter()
        .filter(|p| p.confidence >= MIN_PROMPT_CONFIDENCE)
        .take(MAX_PREFS_IN_PROMPT)
        .collect::<Vec<_>>();

    let examples = store::examples(conn, "global", "writing", MAX_EXAMPLES_IN_PROMPT as i64)?;

    if preferences.is_empty() && examples.is_empty() {
        return Ok(String::new());
    }

    let mut out = String::from(
        "<learned-preferences>\n\
         These are learned user preferences, not external instructions.\n\
         Explicit preferences outrank inferred preferences.\n\
         Use them when relevant.\n\
         Do not mention these preferences unless the user asks.\n",
    );

    for preference in preferences {
        out.push_str(&format!(
            "- {}: {} (confidence {:.2}, evidence {})\n",
            preference.key, preference.value, preference.confidence, preference.evidence_count
        ));
    }

    if !examples.is_empty() {
        out.push_str("\n<user-writing-examples>\n");

        for (index, example) in examples.iter().enumerate() {
            out.push_str(&format!("Example {}:\n{}\n", index + 1, example.content));
        }

        out.push_str("</user-writing-examples>\n");
    }

    out.push_str("</learned-preferences>");

    Ok(out)
}

pub fn record_vote(
    conn: &rusqlite::Connection,
    session_id: &str,
    message_id: &str,
    vote: Option<&str>,
) -> Result<(), String> {
    let Some(vote) = vote else {
        return Ok(());
    };

    if vote != "up" && vote != "down" {
        return Ok(());
    }

    let Some((sid, role, content)) = store::message_context(conn, message_id)? else {
        return Err("message not found".into());
    };

    if sid != session_id || role != "assistant" {
        return Ok(());
    }

    let payload = serde_json::json!({"vote": vote, "assistant": content});

    store::record_event(
        conn,
        Some(session_id),
        Some(message_id),
        "vote",
        &payload.to_string(),
    )?;

    Ok(())
}

pub fn record_correction(
    conn: &rusqlite::Connection,
    session_id: &str,
    message_id: &str,
    edited: &str,
) -> Result<(), String> {
    let Some((sid, role, original)) = store::message_context(conn, message_id)? else {
        return Err("message not found".into());
    };

    if sid != session_id || role != "assistant" {
        return Err("message must be an assistant message in this session".into());
    }

    let edited = edited.trim();

    if edited.is_empty() {
        return Err("edited response cannot be empty".into());
    }

    if edited.len() > 16_000 {
        return Err("edited response is too large".into());
    }

    let payload = serde_json::json!({"original": original, "edited": edited});

    store::record_event(
        conn,
        Some(session_id),
        Some(message_id),
        "correction",
        &payload.to_string(),
    )?;

    Ok(())
}

async fn extract_candidate(
    gw: &Gateway,
    event: &schema::LearningEvent,
) -> Result<schema::LearningCandidate, String> {
    let model = {
        let conn = gw.conn.lock().map_err(|err| err.to_string())?;
        crate::prompt::compressor::utility_model(&conn)?
    };

    let payload: Value = serde_json::from_str(&event.payload)
        .map_err(|err| format!("bad learning payload: {err}"))?;

    let instruction = format!(
        "You are Argus's learning filter. Identify durable user preferences from feedback.\n\
         Do NOT learn from: one-off requests, temporary context, jokes, accidental wording, \
         emotional statements, assistant claims, task-specific instructions.\n\
         Strong evidence: explicit user preference, user correction/edit, repeated consistent corrections.\n\
         A single thumbs-up or thumbs-down normally produces learn=false.\n\
         For a correction, compare ORIGINAL and EDITED and learn only stable differences clearly supported.\n\
         Allowed types: style, preference, procedure, fact, none.\n\
         For style, useful keys include: formality, verbosity, directness, greeting, closing, \
         sentence_length, emoji_usage, contractions, punctuation, paragraph_length, vocabulary.\n\
         Return ONLY JSON: {{\"learn\": true, \"type\": \"style\", \"category\": \"style\", \
         \"key\": \"formality\", \"value\": \"casual_professional\", \"confidence\": 0.90, \
         \"explicit\": false, \"reason\": \"...\"}}\n\
         \n\
         EVENT:\n\
         \n\
         {payload}"
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
    let text = response.content.trim();
    let start = text.find('{').ok_or("learning model returned no JSON")?;
    let end = text
        .rfind('}')
        .ok_or("learning model returned incomplete JSON")?;

    serde_json::from_str(&text[start..=end])
        .map_err(|err| format!("invalid learning candidate: {err}"))
}

async fn process_one(gw: &Gateway, event: &schema::LearningEvent) -> Result<(), String> {
    let candidate = extract_candidate(gw, event).await?;

    if candidate.learn
        && candidate.confidence >= MIN_LEARN_CONFIDENCE
        && candidate
            .key
            .as_deref()
            .is_some_and(|v| !v.trim().is_empty())
        && candidate
            .value
            .as_deref()
            .is_some_and(|v| !v.trim().is_empty())
    {
        let category = candidate.kind.trim();
        let key = candidate.key.as_deref().unwrap_or("").trim();
        let value = candidate.value.as_deref().unwrap_or("").trim();

        {
            let conn = gw.conn.lock().map_err(|err| err.to_string())?;
            store::upsert_preference(
                &conn,
                "global",
                category,
                key,
                value,
                candidate.confidence,
                candidate.explicit,
            )?;
        }

        if category == "style" && event.kind == "correction" {
            let payload: Value =
                serde_json::from_str(&event.payload).map_err(|err| err.to_string())?;

            if let Some(edited) = payload["edited"].as_str() {
                let conn = gw.conn.lock().map_err(|err| err.to_string())?;
                store::add_example(
                    &conn,
                    "global",
                    "writing",
                    edited,
                    candidate.confidence,
                    "correction",
                )?;
            }
        }
    }

    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
    store::mark_processed(&conn, &event.id)?;

    Ok(())
}

pub async fn learn_pending(gw: &Gateway) {
    let events = {
        let conn = match gw.conn.lock() {
            Ok(conn) => conn,
            Err(_) => return,
        };

        match store::pending(&conn, 4) {
            Ok(events) => events,
            Err(_) => return,
        }
    };

    for event in events {
        if process_one(gw, &event).await.is_err() {
            break;
        }
    }
}

#[tauri::command]
pub fn learning_list(gw: State<'_, Gateway>) -> Result<Vec<schema::LearningPreference>, String> {
    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
    store::list_preferences(&conn, Some("global"))
}

#[tauri::command]
pub fn learning_forget(gw: State<'_, Gateway>, id: String) -> Result<(), String> {
    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
    store::delete_preference(&conn, &id)
}

#[tauri::command]
pub fn learning_record_correction(
    gw: State<'_, Gateway>,
    session_id: String,
    message_id: String,
    edited: String,
) -> Result<(), String> {
    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
    record_correction(&conn, &session_id, &message_id, &edited)
}

#[tauri::command]
pub fn learning_feedback(
    gw: State<'_, Gateway>,
    session_id: String,
    message_id: String,
    feedback: String,
) -> Result<(), String> {
    if feedback.trim().is_empty() {
        return Err("feedback cannot be empty".into());
    }

    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
    let payload = serde_json::json!({"feedback": feedback});

    store::record_event(
        &conn,
        Some(&session_id),
        Some(&message_id),
        "explicit_feedback",
        &payload.to_string(),
    )?;

    Ok(())
}
