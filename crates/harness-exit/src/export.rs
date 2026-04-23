// IMPLEMENTS: D-418
//! `harness export` envelope. Carries the user's events, memory,
//! consent ledger AND the Agent Trace sidecar pinned by digest so
//! the bundle can be replayed on a fresh daemon (GDPR Art 20).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExportBundle {
    pub schema: String,
    pub principal_id: String,
    pub generated_at_iso: String,
    pub events_blob: Vec<u8>,
    pub memory_blob: Vec<u8>,
    pub consent_vc: serde_json::Value,
    /// Agent Trace blob digest — caller persists the trace alongside
    /// the bundle and pins by this hex hash.
    pub agent_trace_blob_hex: String,
}

#[must_use]
pub fn build_export_bundle(
    principal_id: impl Into<String>,
    generated_at_iso: impl Into<String>,
    events_blob: Vec<u8>,
    memory_blob: Vec<u8>,
    consent_vc: serde_json::Value,
    agent_trace_bytes: &[u8],
) -> ExportBundle {
    let mut h = blake3::Hasher::new();
    h.update(b"harness/export/agent-trace/v1\n");
    h.update(agent_trace_bytes);
    let pin = h.finalize().to_hex().to_string();
    ExportBundle {
        schema: "harness/exit/export/v1".into(),
        principal_id: principal_id.into(),
        generated_at_iso: generated_at_iso.into(),
        events_blob,
        memory_blob,
        consent_vc,
        agent_trace_blob_hex: pin,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundle_pins_trace_digest() {
        let b = build_export_bundle(
            "p-1",
            "2026-04-22",
            vec![1, 2],
            vec![3],
            serde_json::json!({"vc": true}),
            b"trace",
        );
        assert_eq!(b.agent_trace_blob_hex.len(), 64);
        assert!(!b.agent_trace_blob_hex.is_empty());
    }

    #[test]
    fn pin_is_deterministic() {
        let a = build_export_bundle("p", "t", vec![], vec![], serde_json::json!({}), b"trace");
        let b = build_export_bundle("p", "t", vec![], vec![], serde_json::json!({}), b"trace");
        assert_eq!(a.agent_trace_blob_hex, b.agent_trace_blob_hex);
    }

    #[test]
    fn schema_label_is_pinned() {
        let b = build_export_bundle("p", "t", vec![], vec![], serde_json::json!({}), b"x");
        assert_eq!(b.schema, "harness/exit/export/v1");
    }
}
