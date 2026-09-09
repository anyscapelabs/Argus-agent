use rusqlite::params;
use tauri::ipc::Channel;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::gateway::router;
use crate::gateway::schema::{ChatReq, StreamEvent, WireMsg};
use crate::gateway::Gateway;
use crate::prompt::config::truncate_chars;
use crate::prompt::{compressor, project};
use crate::tools;

use super::schema::NewMsg;
use super::store;

const DEFAULT_TITLE: &str = "New chat";
const MAX_STEPS: usize = 8;
const RESULT_CLIP: usize = 4000;

const TITLE_SYS: &str = "You are the title generator for Argus, a personal AI agent the user chats with. \
Write a short session title for the user's message. Reply with only the title: \
3 to 6 words, no quotes, no trailing punctuation.";

// Title lives in the detached task so the turn never waits on it. The user
// message already serves as the title until this lands.
async fn generate_title(gw: &Gateway, session_id: &str, content: &str) -> Result<(), String> {
  let (util, selected) = {
    let conn = gw.conn.lock().map_err(|e| e.to_string())?;
    let selected: Option<String> = conn
      .query_row("SELECT model_id FROM sessions WHERE id = ?1", params![session_id], |r| r.get(0))
      .map_err(|e| e.to_string())?;
    (compressor::utility_model(&conn), selected)
  };

  let msgs = vec![
    WireMsg { role: "system".into(), content: TITLE_SYS.into() },
    WireMsg { role: "user".into(), content: truncate_chars(content, 500) },
  ];
  let raw = match util {
    Ok(u) => {
      let req = ChatReq { model: u, msgs: msgs.clone(), prefix_hash: None };
      match router::run_opts(gw, &req, 2).await {
        Ok(resp) => resp.content,
        Err(_) => fallback_title(gw, selected, &msgs).await?,
      }
    }
    Err(_) => fallback_title(gw, selected, &msgs).await?,
  };

  let title = clean_title(&raw).ok_or("title model returned nothing usable")?;
  let conn = gw.conn.lock().map_err(|e| e.to_string())?;
  conn
    .execute(
      "UPDATE sessions SET title = ?2 WHERE id = ?1",
      params![session_id, title],
    )
    .map_err(|e| e.to_string())?;
  Ok(())
}

// Utility call failed or was unavailable: retry on the session's own model.
async fn fallback_title(
  gw: &Gateway,
  selected: Option<String>,
  msgs: &[WireMsg],
) -> Result<String, String> {
  let model = selected.filter(|m| !m.is_empty()).ok_or("no fallback model for title")?;
  let req = ChatReq { model, msgs: msgs.to_vec(), prefix_hash: None };
  Ok(router::run_opts(gw, &req, 2).await?.content)
}

fn clean_title(raw: &str) -> Option<String> {
  let line = raw.lines().next().unwrap_or("").trim();
  let t = line.trim_matches('"').trim_matches('\'').trim();
  if t.is_empty() {
    return None;
  }
  Some(truncate_chars(t, 60))
}

// One turn, possibly many steps: stream a reply, run any <action> blocks it
// carries, feed the results back, repeat until a plain answer lands or the
// step budget is spent.
pub async fn send(
  gw: &Gateway,
  app: &AppHandle,
  session_id: &str,
  content: &str,
  chan: &Channel<StreamEvent>,
) -> Result<(), String> {
  {
    let conn = gw.conn.lock().map_err(|e| e.to_string())?;
    store::add_msg(
      &conn,
      &NewMsg {
        session_id: session_id.into(),
        role: "user".into(),
        content: content.into(),
        model_id: None,
        provider_id: None,
        tok_in: None,
        tok_out: None,
      },
    )?;
  }

  let perm = {
    let conn = gw.conn.lock().map_err(|e| e.to_string())?;
    conn
      .query_row("SELECT permission FROM sessions WHERE id = ?1", params![session_id], |r| {
        r.get::<_, String>(0)
      })
      .unwrap_or_else(|_| "ask".into())
  };

  let mut tok_in_sum = 0i64;

  for _step in 0..MAX_STEPS {
    let mut req = {
      let conn = gw.conn.lock().map_err(|e| e.to_string())?;
      let p = project(&conn, session_id)?;
      if p.model_id.is_none() {
        return Err("session has no model set".into()); // Drop it
      }
      let mut r = p.chat_req();
      r.prefix_hash = Some(p.prefix_hash);
      r
    };

    let stats = router::stream_run(gw, &req, chan).await?;
    req.msgs.clear(); // release transcript memory before the next step
    tok_in_sum += stats.tok_in;

    // Empty completion: free models flake like this. Failing the turn beats a
    // silent end where only a spinner shows.
    if stats.text.trim().is_empty() {
        return Err("model returned an empty reply — try again".into());
    }

    let actions = tools::parse_actions(&stats.text);
    let done = actions.is_empty();

    {
      let conn = gw.conn.lock().map_err(|e| e.to_string())?;
      store::add_msg(
        &conn,
        &NewMsg {
          session_id: session_id.into(),
          role: "assistant".into(),
          content: stats.text,
          model_id: Some(stats.model_id),
          provider_id: Some(stats.provider_id),
          tok_in: Some(stats.tok_in),
          tok_out: Some(stats.tok_out),
        },
      )?;
    }

    if done {
      break;
    }

    // Step boundary: UI swaps the streamed text for the persisted rows, so
    // finished steps read "Ran" while only the live one shows "Running".
    let _ = chan.send(StreamEvent::Step);

    for a in &actions {
      let (status, body) = match tools::exec(&a.tool, &a.args, &perm).await {
        Ok(t) => ("ok", t),
        Err(e) => ("err", e),
      };
      let msg = format!(
        "<tool-result tool=\"{}\" status=\"{}\">{}</tool-result>",
        a.tool,
        status,
        truncate_chars(&body, RESULT_CLIP)
      );
      let conn = gw.conn.lock().map_err(|e| e.to_string())?;
      store::add_msg(
        &conn,
        &NewMsg {
          session_id: session_id.into(),
          role: "user".into(),
          content: msg,
          model_id: None,
          provider_id: None,
          tok_in: None,
          tok_out: None,
        },
      )?;
    }
  }

  {
    let conn = gw.conn.lock().map_err(|e| e.to_string())?;
    store::touch_session(&conn, session_id, tok_in_sum)?;
  }

  let needs = {
    let conn = gw.conn.lock().map_err(|e| e.to_string())?;
    compressor::check(&conn, session_id).map(|s| s.needs_compact).unwrap_or(false)
  };
  if needs {
    if let Err(e) = compressor::compact(gw, session_id).await {
      eprintln!("compaction skipped: {e}"); // next turn tries again
    }
  }

  // Untitled session: generate a title off-turn. Utility (free) model first,
  // the session's selected model as fallback; failure keeps "New chat".
  let untitled = {
    let conn = gw.conn.lock().map_err(|e| e.to_string())?;
    conn
      .query_row("SELECT title FROM sessions WHERE id = ?1", params![session_id], |r| {
        r.get::<_, String>(0)
      })
      .map(|t| t == DEFAULT_TITLE)
      .unwrap_or(false)
  };
  if untitled {
    // The user message stands in as the title until the model returns one.
    let temp = clean_title(content).unwrap_or_else(|| DEFAULT_TITLE.into());
    {
      let conn = gw.conn.lock().map_err(|e| e.to_string())?;
      conn
        .execute(
          "UPDATE sessions SET title = ?2 WHERE id = ?1",
          params![session_id, temp],
        )
        .map_err(|e| e.to_string())?;
    }
    let app = app.clone();
    let sid = session_id.to_string();
    let user_text = content.to_string();
    tauri::async_runtime::spawn(async move {
      let gw = app.state::<Gateway>();
      if let Err(e) = generate_title(gw.inner(), &sid, &user_text).await {
        eprintln!("title skipped: {e}"); // keeps the user-message title
      }
      let _ = app.emit("sessions-changed", ());
    });
  }
  Ok(())
}

#[tauri::command]
pub async fn sess_chat_stream(
  gw: State<'_, Gateway>,
  app: AppHandle,
  session_id: String,
  content: String,
  on_event: Channel<StreamEvent>,
) -> Result<(), String> {
  send(&gw, &app, &session_id, &content, &on_event).await
}
