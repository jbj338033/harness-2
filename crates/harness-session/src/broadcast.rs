use harness_core::{AgentId, MessageId, SessionId, ToolCallId};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;
use tokio::sync::broadcast;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SessionEvent {
    MessageCreated {
        agent_id: AgentId,
        message_id: MessageId,
        role: String,
    },
    MessageDelta {
        message_id: MessageId,
        content: String,
    },
    MessageDone {
        message_id: MessageId,
    },
    AgentStatus {
        agent_id: AgentId,
        status: String,
    },
    Error {
        reason: String,
    },
    ToolCallStart {
        message_id: MessageId,
        tool_call_id: ToolCallId,
        name: String,
        input_preview: String,
    },
    ToolCallResult {
        tool_call_id: ToolCallId,
        output: String,
        is_error: bool,
    },
    ApprovalRequest {
        request_id: String,
        command: String,
        pattern: String,
        reason: String,
    },
}

pub struct SessionBroadcaster {
    senders: Mutex<HashMap<SessionId, broadcast::Sender<SessionEvent>>>,
    capacity: usize,
}

impl SessionBroadcaster {
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Self {
            senders: Mutex::new(HashMap::new()),
            capacity,
        }
    }

    pub fn sender(&self, session: SessionId) -> broadcast::Sender<SessionEvent> {
        let mut map = self.senders.lock().expect("mutex poisoned");
        map.entry(session)
            .or_insert_with(|| broadcast::channel(self.capacity).0)
            .clone()
    }

    pub fn subscribe(&self, session: SessionId) -> broadcast::Receiver<SessionEvent> {
        self.sender(session).subscribe()
    }

    pub fn publish(&self, session: SessionId, event: SessionEvent) -> usize {
        let sender = self.sender(session);
        sender.send(event).unwrap_or(0)
    }

    #[must_use]
    pub fn session_count(&self) -> usize {
        self.senders.lock().expect("mutex poisoned").len()
    }

    #[must_use]
    pub fn total_subscribers(&self) -> usize {
        self.senders
            .lock()
            .unwrap()
            .values()
            .map(broadcast::Sender::receiver_count)
            .sum()
    }

    pub fn drop_session(&self, session: SessionId) {
        self.senders
            .lock()
            .expect("mutex poisoned")
            .remove(&session);
    }
}

impl Default for SessionBroadcaster {
    fn default() -> Self {
        Self::new(128)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn one_publisher_many_subscribers() {
        let b = SessionBroadcaster::new(16);
        let sid = SessionId::new();
        let mut rx1 = b.subscribe(sid);
        let mut rx2 = b.subscribe(sid);

        assert_eq!(
            b.publish(
                sid,
                SessionEvent::MessageDelta {
                    message_id: MessageId::new(),
                    content: "hi".into(),
                }
            ),
            2
        );

        match rx1.recv().await.unwrap() {
            SessionEvent::MessageDelta { content, .. } => assert_eq!(content, "hi"),
            _ => panic!(),
        }
        match rx2.recv().await.unwrap() {
            SessionEvent::MessageDelta { content, .. } => assert_eq!(content, "hi"),
            _ => panic!(),
        }
    }

    #[tokio::test]
    async fn late_subscribers_miss_history() {
        let b = SessionBroadcaster::new(16);
        let sid = SessionId::new();
        b.publish(
            sid,
            SessionEvent::MessageDelta {
                message_id: MessageId::new(),
                content: "early".into(),
            },
        );
        let mut rx = b.subscribe(sid);
        b.publish(
            sid,
            SessionEvent::MessageDelta {
                message_id: MessageId::new(),
                content: "late".into(),
            },
        );
        match rx.recv().await.unwrap() {
            SessionEvent::MessageDelta { content, .. } => assert_eq!(content, "late"),
            _ => panic!(),
        }
    }

    #[tokio::test]
    async fn different_sessions_are_isolated() {
        let b = SessionBroadcaster::new(16);
        let s1 = SessionId::new();
        let s2 = SessionId::new();
        let mut rx1 = b.subscribe(s1);
        let _rx2 = b.subscribe(s2);

        assert_eq!(
            b.publish(
                s2,
                SessionEvent::MessageDelta {
                    message_id: MessageId::new(),
                    content: "for-s2".into(),
                }
            ),
            1
        );
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(50), rx1.recv())
                .await
                .is_err()
        );
    }

    #[test]
    fn drop_session_removes_sender() {
        let b = SessionBroadcaster::new(16);
        let sid = SessionId::new();
        b.sender(sid);
        assert_eq!(b.session_count(), 1);
        b.drop_session(sid);
        assert_eq!(b.session_count(), 0);
    }

    #[test]
    fn session_event_roundtrip() {
        let e = SessionEvent::MessageDelta {
            message_id: MessageId::new(),
            content: "x".into(),
        };
        let s = serde_json::to_string(&e).unwrap();
        let back: SessionEvent = serde_json::from_str(&s).unwrap();
        match back {
            SessionEvent::MessageDelta { content, .. } => assert_eq!(content, "x"),
            _ => panic!(),
        }
    }
}
