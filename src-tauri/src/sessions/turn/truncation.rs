use super::{Truncation, Turn};
use crate::gateway::router;
use crate::gateway::schema::StreamEvent;
use crate::gateway::Gateway;
use crate::sessions::chat::{MAX_TRUNC_CONTS, TRUNC_CONT};
use crate::sessions::schema::Msg;
use crate::sessions::{sink, store};

/// A repeat of the same words counts against the limit: asking again only buys
/// another copy, which is how one answer lands in the chat three times. The
/// notice goes out only when the model really was cut off.
impl Turn {
    pub(super) fn handle_truncation(
        &mut self,
        gw: &Gateway,
        asst: &Msg,
        sink: &dyn sink::ChatSink,
        stats: &router::StreamStats,
        done: bool,
        said_again: bool,
    ) -> Truncation {
        if !stats.truncated {
            return Truncation::None;
        }

        self.trunc_conts += 1;

        if self.trunc_conts > MAX_TRUNC_CONTS || said_again {
            if !done {
                return Truncation::Overflow;
            }

            if !said_again {
                sink.emit(StreamEvent::Notice {
                    msg: "the model's reply was cut off at its output limit twice — \
                          partial work above is saved; send 'continue' to resume"
                        .into(),
                });
            }

            if let Ok(conn) = gw.conn.lock() {
                let _ = store::mark_final(&conn, &asst.id);
            }

            self.finished = true;
            return Truncation::Finish;
        }

        self.nudge = Some(TRUNC_CONT.into());

        if done {
            Truncation::Retry
        } else {
            Truncation::None
        }
    }
}
