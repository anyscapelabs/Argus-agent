// Writing the assistant's turn to the transcript.

use super::Turn;
use crate::gateway::router;
use crate::gateway::Gateway;
use crate::sessions::schema::{Msg, NewMsg};
use crate::sessions::store;

/// Write the assistant's turn to the transcript.
///
/// `add_msg_dedup` reports whether this step merely repeated the previous
/// one, which the caller needs because a repeat is not a truncation and
/// must not be reported as one.
impl Turn {
    pub(super) fn persist_assistant(
        &self,
        gw: &Gateway,
        session_id: &str,
        text: &str,
        model_id: &str,
        stats: &router::StreamStats,
        tool_calls: Option<String>,
    ) -> Result<(Msg, bool), String> {
        let conn = gw.conn.lock().map_err(|err| err.to_string())?;
        store::add_msg_dedup(
            &conn,
            &NewMsg {
                session_id: session_id.into(),
                role: "assistant".into(),
                content: text.to_string(),
                model_id: Some(model_id.to_string()),
                provider_id: Some(stats.provider_id.clone()),
                tok_in: Some(stats.tok_in),
                tok_out: Some(stats.tok_out),
                tool_calls,
                tool_call_id: None,
                attachments: None,
            },
        )
    }
}
