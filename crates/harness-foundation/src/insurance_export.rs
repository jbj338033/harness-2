// IMPLEMENTS: D-430
//! 90-day Agent Trace retention + insurance / legal export envelope.
//! The Armilla + Chaucer 2025-04 dedicated AI-liability policy was
//! the forcing function — insurers want a portable audit trail so
//! they can underwrite the risk.

use serde::{Deserialize, Serialize};

pub const AGENT_TRACE_RETENTION_DAYS: u32 = 90;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InsuranceExport {
    pub schema: String,
    pub principal_id: String,
    pub trace_window_start_iso: String,
    pub trace_window_end_iso: String,
    pub trace_blob: Vec<u8>,
    pub policy_reference: Option<String>,
}

#[must_use]
pub fn build_insurance_export(
    principal_id: impl Into<String>,
    trace_window_start_iso: impl Into<String>,
    trace_window_end_iso: impl Into<String>,
    trace_blob: Vec<u8>,
    policy_reference: Option<String>,
) -> InsuranceExport {
    InsuranceExport {
        schema: "harness/foundation/insurance/v1".into(),
        principal_id: principal_id.into(),
        trace_window_start_iso: trace_window_start_iso.into(),
        trace_window_end_iso: trace_window_end_iso.into(),
        trace_blob,
        policy_reference,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retention_is_ninety_days() {
        assert_eq!(AGENT_TRACE_RETENTION_DAYS, 90);
    }

    #[test]
    fn export_round_trips() {
        let e = build_insurance_export(
            "p-1",
            "2026-01-01",
            "2026-04-01",
            vec![1, 2, 3],
            Some("CHAUCER-AI-LIABILITY-001".into()),
        );
        let s = serde_json::to_string(&e).unwrap();
        let back: InsuranceExport = serde_json::from_str(&s).unwrap();
        assert_eq!(back, e);
    }

    #[test]
    fn schema_label_is_pinned() {
        let e = build_insurance_export("p", "a", "b", vec![], None);
        assert_eq!(e.schema, "harness/foundation/insurance/v1");
    }
}
