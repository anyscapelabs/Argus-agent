use rusqlite::params;

use super::Turn;
use crate::gateway::schema::StreamEvent;
use crate::gateway::Gateway;
use crate::sessions::{blocks, guards, sink, store};

/// Leaves a resume and warns on the last final message, so the "send continue"
/// the notice promises is one the next turn can keep.
impl Turn {
    pub(super) fn budget_stopped(
        &mut self,
        gw: &Gateway,
        session_id: &str,
        sink: &dyn sink::ChatSink,
        turn_budget: i64,
    ) -> bool {
        if guards::budget_state(self.tok_in_sum, turn_budget) != guards::Budget::Stop {
            return false;
        }

        blocks::save_resume(gw, session_id, &self.turn_actions);

        if let Ok(conn) = gw.conn.lock() {
            let _ = crate::playbook::store::record(
                &conn,
                crate::playbook::Kind::BudgetStop,
                &crate::sessions::ext_install::host_id(),
                Some(session_id),
                &format!("{} of {turn_budget} tokens", self.tok_in_sum),
            );
        }
        if let Ok(conn) = gw.conn.lock() {
            if let Ok(last) = store::get_last_final(&conn, session_id) {
                let _ = conn.execute(
                    "UPDATE messages SET content = content || ?2 WHERE id = ?1",
                    params![
                        &last,
                        "\n<warning severity=\"medium\">this turn hit its budget — the work \
                         above is saved; send 'continue' to pick it up in a new turn</warning>"
                    ],
                );
                let _ = store::mark_final(&conn, &last);
            }
        }
        sink.emit(StreamEvent::Notice {
            msg: "this turn hit its budget — the work above is saved; send 'continue' to \
                  pick it up in a new turn"
                .into(),
        });
        self.finished = true;
        true
    }
}
