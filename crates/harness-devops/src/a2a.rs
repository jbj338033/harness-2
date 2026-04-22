// IMPLEMENTS: D-259
//! SRE A2A (agent-to-agent) endpoint descriptor. Datadog Bits and
//! Azure SRE-Copilot get host-side orchestrated through these
//! endpoints — harness keeps the policy boundary, the foreign agent
//! provides the domain skill.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum A2aProvider {
    DatadogBits,
    AzureSreCopilot,
    Custom,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct A2aEndpoint {
    pub provider: A2aProvider,
    pub url: String,
    pub agent_card_uri: String,
    /// Daemon-controlled — the foreign agent runs only when
    /// `enabled = true` and the operator has approved its `AgentCard`.
    pub enabled: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_round_trips() {
        let e = A2aEndpoint {
            provider: A2aProvider::DatadogBits,
            url: "https://example.com/a2a".into(),
            agent_card_uri: "https://example.com/agent.json".into(),
            enabled: false,
        };
        let s = serde_json::to_string(&e).unwrap();
        let back: A2aEndpoint = serde_json::from_str(&s).unwrap();
        assert_eq!(back, e);
        assert!(s.contains("datadog_bits"));
    }

    #[test]
    fn endpoint_disabled_by_default() {
        let e = A2aEndpoint {
            provider: A2aProvider::Custom,
            url: "x".into(),
            agent_card_uri: "x".into(),
            enabled: false,
        };
        assert!(!e.enabled);
    }
}
