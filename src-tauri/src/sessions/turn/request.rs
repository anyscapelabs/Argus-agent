// Building the request for one step.

use super::Turn;
use crate::gateway::schema::{ChatReq, WireMsg};
use crate::gateway::Gateway;
use crate::prompt::project;
use crate::sessions::blocks;
use crate::sessions::chat::auto_model;

/// Build the request for one step: project the transcript, resolve a model
/// if the session pinned none, and spend any pending nudge as a user
/// message. Taking the nudge here is what makes it single-use — a nudge
/// queued but not delivered would otherwise ride along forever.
impl Turn {
    pub(super) fn build_request(
        &mut self,
        gw: &Gateway,
        session_id: &str,
    ) -> Result<ChatReq, String> {
        let conn = gw.conn.lock().map_err(|err| err.to_string())?;
        let mut p = project(&conn, session_id, &gw.library_dir)?;
        if p.model_id.is_none() {
            p.model_id = Some(auto_model(&conn)?);
        }

        let mut r = p.chat_req();
        blocks::attach_shots(&mut r.msgs);

        if let Some(n) = self.nudge.take() {
            r.msgs.push(WireMsg {
                role: "user".into(),
                content: n,
                ..Default::default()
            });
        }

        r.prefix_hash = Some(p.prefix_hash);
        Ok(r)
    }
}
