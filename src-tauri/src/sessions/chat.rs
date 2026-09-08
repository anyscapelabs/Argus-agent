use tauri::ipc::Channel;
use tauri::State;

use crate::gateway::router;
use crate::gateway::schema::StreamEvent;
use crate::gateway::Gateway;
use crate::prompt::{compressor, project};

use super::store;

// One turn end to end: persist the user message, project, stream from the
// gateway, persist the reply, then compact when the context says so.
pub async fn send(
  gw: &Gateway,
  session_id: &str,
  content: &str,
  chan: &Channel<StreamEvent>,
) -> Result<(), String> {
  {
    let conn = gw.conn.lock().map_err(|e| e.to_string())?;
    store::add_msg(
      &conn,
      &super::schema::NewMsg {
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
  req.msgs.clear(); // release transcript memory before compaction

  {
    let conn = gw.conn.lock().map_err(|e| e.to_string())?;
    store::add_msg(
      &conn,
      &super::schema::NewMsg {
        session_id: session_id.into(),
        role: "assistant".into(),
        content: stats.text,
        model_id: Some(stats.model_id),
        provider_id: Some(stats.provider_id),
        tok_in: Some(stats.tok_in),
        tok_out: Some(stats.tok_out),
      },
    )?;
    store::touch_session(&conn, session_id, stats.tok_in)?;
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
  Ok(())
}

#[tauri::command]
pub async fn sess_chat_stream(
  gw: State<'_, Gateway>,
  session_id: String,
  content: String,
  on_event: Channel<StreamEvent>,
) -> Result<(), String> {
  send(&gw, &session_id, &content, &on_event).await
}
