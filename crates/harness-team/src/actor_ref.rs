// IMPLEMENTS: D-240, D-248
//! `ActorRef` — addressable target for `Speak.to`. `Broadcast` is
//! the wave-fanout target.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct CorrelationId(pub String);

impl CorrelationId {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActorKind {
    Main,
    Subagent,
    Worker,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ActorRef {
    Main,
    Subagent {
        id: String,
    },
    Worker {
        id: String,
    },
    /// All active actors in the wave with this `CorrelationId`.
    Broadcast {
        correlation: CorrelationId,
    },
}

impl ActorRef {
    #[must_use]
    pub fn is_broadcast(&self) -> bool {
        matches!(self, ActorRef::Broadcast { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn broadcast_target_is_recognised() {
        let r = ActorRef::Broadcast {
            correlation: CorrelationId::new("c1"),
        };
        assert!(r.is_broadcast());
    }

    #[test]
    fn subagent_actor_round_trips() {
        let r = ActorRef::Subagent { id: "code".into() };
        let s = serde_json::to_string(&r).unwrap();
        assert!(s.contains("\"kind\":\"subagent\""));
        let back: ActorRef = serde_json::from_str(&s).unwrap();
        assert_eq!(back, r);
    }
}
