// Where a turn's events go.
//
// The channel is the normal path. The bus exists so a watcher that attached
// mid-turn still sees the events that follow it, and the fan keeps both.

use tauri::ipc::Channel;

use crate::gateway::schema::StreamEvent;
use crate::gateway::{EventSink, Gateway};

pub trait ChatSink: Send + Sync {
    fn emit(&self, ev: StreamEvent);

    /// How a spawned task — a running command's output, the model's stream —
    /// gets events back here. `None` only if this sink genuinely wants them
    /// dropped; a bus sink must answer, or the chat goes silent for exactly
    /// the events a user is waiting on.
    fn event_sink(&self) -> Option<EventSink> {
        None
    }

    fn detached(&self) -> bool {
        false
    }
}

impl ChatSink for Channel<StreamEvent> {
    fn emit(&self, ev: StreamEvent) {
        let _ = self.send(ev);
    }

    fn event_sink(&self) -> Option<EventSink> {
        Some(EventSink::Channel(self.clone()))
    }
}

pub struct NullSink;

impl ChatSink for NullSink {
    fn emit(&self, _ev: StreamEvent) {}
}

pub struct BusSink<'a> {
    pub gw: &'a Gateway,
    pub session_id: String,
}

impl ChatSink for BusSink<'_> {
    fn emit(&self, ev: StreamEvent) {
        self.gw.publish(&self.session_id, ev);
    }

    fn event_sink(&self) -> Option<EventSink> {
        Some(EventSink::Bus(self.gw.term_tx(&self.session_id)))
    }

    fn detached(&self) -> bool {
        !self.gw.watched(&self.session_id)
    }
}

/// A sub-agent runs in its own session but answers to the chat that spawned
/// it. Only an approval crosses into the parent, because a human has to be
/// able to answer it and the parent chat is where approvals are answered.
/// Everything else — deltas, terminal output, and above all TurnEnd, which
/// would end the parent's turn out from under it — stays in the child.
///
/// The one thing it will not do is ask when nobody is there. `detached` keys
/// off the parent being attached, so the moment that window closes the agent
/// fails closed exactly like any other unattended turn.
pub struct FanSink<'a> {
    pub gw: &'a Gateway,
    pub child_id: String,
    pub parent_id: String,
}

impl ChatSink for FanSink<'_> {
    /// Only an approval crosses into the parent, because a human has to be
    /// able to answer it and the parent chat is where approvals are answered.
    /// Everything else — deltas, terminal output, and above all TurnEnd, which
    /// would tear down the parent's own turn — stays on the child's own bus.
    fn emit(&self, ev: StreamEvent) {
        if matches!(ev, StreamEvent::Approval { .. }) {
            self.gw.publish(&self.parent_id, ev.clone());
        }

        self.gw.publish(&self.child_id, ev);
    }

    fn event_sink(&self) -> Option<EventSink> {
        Some(EventSink::Bus(self.gw.term_tx(&self.child_id)))
    }

    fn detached(&self) -> bool {
        !self.gw.attached(&self.parent_id)
    }
}
