// IMPLEMENTS: D-246
//! `mailboxes` projection row — fast lookup of who sent what to whom.
//! Backed by the events table; this row is the materialised view
//! the Web/TUI uses to render conversation history per actor pair.

use crate::actor_ref::ActorRef;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MailboxRow {
    pub actor_id: String,
    pub from: ActorRef,
    pub to: ActorRef,
    pub event_id: String,
    pub read_at_ms: Option<i64>,
}

impl MailboxRow {
    pub fn mark_read(&mut self, at_ms: i64) {
        self.read_at_ms = Some(at_ms);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mark_read_sets_timestamp() {
        let mut m = MailboxRow {
            actor_id: "a".into(),
            from: ActorRef::Main,
            to: ActorRef::Subagent { id: "s".into() },
            event_id: "e1".into(),
            read_at_ms: None,
        };
        m.mark_read(42);
        assert_eq!(m.read_at_ms, Some(42));
    }
}
