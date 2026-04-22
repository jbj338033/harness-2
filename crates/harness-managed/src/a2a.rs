// IMPLEMENTS: D-155
//! A2A (Agent-to-Agent) protocol envelope. Two directions:
//!  * `Outbound` — the local Main delegates to a foreign agent.
//!  * `Inbound` — Harness exposes itself as an A2A server.
//!
//! The signed AgentCard handle (D-317) is required on every request
//! so a dropped registration can't be replayed silently.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum A2aDirection {
    Outbound,
    Inbound,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentCardHandle {
    pub uri: String,
    pub fingerprint_hex: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct A2aRequest {
    pub direction: A2aDirection,
    pub agent_card: AgentCardHandle,
    pub method: String,
    pub params: serde_json::Value,
    pub at_ms: i64,
}

impl A2aRequest {
    /// Refuse the request if the AgentCard fingerprint is empty —
    /// every A2A call must reach the daemon with a verifiable card.
    #[must_use]
    pub fn is_signed(&self) -> bool {
        !self.agent_card.fingerprint_hex.trim().is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(dir: A2aDirection, fp: &str) -> A2aRequest {
        A2aRequest {
            direction: dir,
            agent_card: AgentCardHandle {
                uri: "https://example.com/agent.json".into(),
                fingerprint_hex: fp.into(),
            },
            method: "v1.runbook.investigate".into(),
            params: serde_json::json!({"alert": "billing.5xx"}),
            at_ms: 1,
        }
    }

    #[test]
    fn signed_request_passes() {
        assert!(req(A2aDirection::Outbound, "abc123").is_signed());
    }

    #[test]
    fn unsigned_request_rejected() {
        assert!(!req(A2aDirection::Inbound, "  ").is_signed());
    }

    #[test]
    fn round_trips_via_serde_with_direction_label() {
        let r = req(A2aDirection::Outbound, "x");
        let s = serde_json::to_string(&r).unwrap();
        assert!(s.contains("\"outbound\""));
        let back: A2aRequest = serde_json::from_str(&s).unwrap();
        assert_eq!(back, r);
    }
}
