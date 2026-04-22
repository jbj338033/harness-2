// IMPLEMENTS: D-366
//! BAA (Business Associate Agreement) provider gate. Local providers
//! (on-device or sandboxed) move freely; external providers must have
//! a recorded BAA before PHI can be sent.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderScope {
    /// On-device or sandboxed model — no BAA needed.
    Local,
    /// Crosses the network — BAA required for medical mode.
    External,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BaaRecord {
    pub counterparty: String,
    pub effective_date_iso: String,
    pub expires_iso: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ProviderGate {
    baas: BTreeMap<String, BaaRecord>,
}

impl ProviderGate {
    pub fn record(&mut self, provider_id: impl Into<String>, baa: BaaRecord) {
        self.baas.insert(provider_id.into(), baa);
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum BaaError {
    #[error("external provider {0} has no BAA on file — refused PHI send")]
    Missing(String),
}

pub fn gate_provider(
    gate: &ProviderGate,
    provider_id: &str,
    scope: ProviderScope,
) -> Result<(), BaaError> {
    match scope {
        ProviderScope::Local => Ok(()),
        ProviderScope::External => {
            if gate.baas.contains_key(provider_id) {
                Ok(())
            } else {
                Err(BaaError::Missing(provider_id.to_string()))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_provider_passes_without_baa() {
        let g = ProviderGate::default();
        assert!(gate_provider(&g, "ollama-local", ProviderScope::Local).is_ok());
    }

    #[test]
    fn external_without_baa_refused() {
        let g = ProviderGate::default();
        assert_eq!(
            gate_provider(&g, "openai", ProviderScope::External),
            Err(BaaError::Missing("openai".into()))
        );
    }

    #[test]
    fn external_with_baa_ok() {
        let mut g = ProviderGate::default();
        g.record(
            "anthropic",
            BaaRecord {
                counterparty: "Anthropic PBC".into(),
                effective_date_iso: "2026-01-01".into(),
                expires_iso: None,
            },
        );
        assert!(gate_provider(&g, "anthropic", ProviderScope::External).is_ok());
    }
}
