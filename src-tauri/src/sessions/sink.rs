// The channel is the normal path. The bus exists so a watcher that attached
// mid-turn still sees the events that follow it, and the fan keeps both.

use tauri::ipc::Channel;

use crate::gateway::schema::StreamEvent;
use crate::gateway::{EventSink, Gateway};

pub trait ChatSink: Send + Sync {
    fn emit(&self, ev: StreamEvent);

    /// For spawned tasks. None drops the events.
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

/// Only an approval crosses into the parent, because that is where approvals
/// are answered. Everything else stays in the child — above all TurnEnd,
/// which would end the parent's turn out from under it.
///
/// Never asks when nobody is there: it fails closed.
pub struct FanSink<'a> {
    pub gw: &'a Gateway,
    pub child_id: String,
    pub parent_id: String,
}

impl ChatSink for FanSink<'_> {
    /// Everything else stays on the child's own bus.
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
