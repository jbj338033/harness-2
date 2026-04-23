// IMPLEMENTS: D-245
//! `Contract(kind=Topic)` — append-only pub-sub bus. Each topic is
//! a projection on top of the events stream; subscribers read by
//! filtering events with that topic name.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TopicContract {
    pub topic_name: String,
    pub subscribers: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TopicMessage {
    pub topic_name: String,
    pub event_id: String,
    pub payload: serde_json::Value,
    pub at_ms: i64,
}

impl TopicContract {
    pub fn subscribe(&mut self, actor_id: impl Into<String>) {
        let id = actor_id.into();
        if !self.subscribers.contains(&id) {
            self.subscribers.push(id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_subscribe_is_idempotent() {
        let mut t = TopicContract {
            topic_name: "alerts".into(),
            subscribers: vec![],
        };
        t.subscribe("a");
        t.subscribe("a");
        t.subscribe("b");
        assert_eq!(t.subscribers, vec!["a", "b"]);
    }

    #[test]
    fn topic_message_round_trips() {
        let m = TopicMessage {
            topic_name: "alerts".into(),
            event_id: "e1".into(),
            payload: serde_json::json!({"x": 1}),
            at_ms: 1,
        };
        let s = serde_json::to_string(&m).unwrap();
        let back: TopicMessage = serde_json::from_str(&s).unwrap();
        assert_eq!(back, m);
    }
}
