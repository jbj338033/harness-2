// IMPLEMENTS: D-374
//! Finance audit event + multi-format export adapters. The supported
//! formats span major regulator schemas: SR 11-7 (Fed model risk),
//! FEAT (HKMA), FRIA (EU AI Act), FINRA, FCA. Each adapter lays the
//! same source event into the schema's required envelope.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditExportFormat {
    Sr117,
    Feat,
    EuFria,
    Finra,
    Fca,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FinanceAuditEvent {
    pub model_id: String,
    pub user_id: String,
    pub task: String,
    pub citations: Vec<String>,
    pub at_iso: String,
}

#[must_use]
pub fn export_audit(event: &FinanceAuditEvent, format: AuditExportFormat) -> serde_json::Value {
    match format {
        AuditExportFormat::Sr117 => serde_json::json!({
            "framework": "SR 11-7",
            "model": event.model_id,
            "use_case": event.task,
            "validator": "harness/local",
            "evidence_refs": event.citations,
            "occurred_at": event.at_iso,
        }),
        AuditExportFormat::Feat => serde_json::json!({
            "framework": "HKMA FEAT",
            "principle": "Effectiveness/Accuracy",
            "model": event.model_id,
            "user": event.user_id,
            "explainability_refs": event.citations,
            "occurred_at": event.at_iso,
        }),
        AuditExportFormat::EuFria => serde_json::json!({
            "framework": "EU AI Act FRIA",
            "system": event.model_id,
            "deployer_user": event.user_id,
            "purpose": event.task,
            "evidence": event.citations,
            "timestamp": event.at_iso,
        }),
        AuditExportFormat::Finra => serde_json::json!({
            "framework": "FINRA Reg Notice 24-09",
            "member_id": event.user_id,
            "activity": event.task,
            "citations": event.citations,
            "ts": event.at_iso,
        }),
        AuditExportFormat::Fca => serde_json::json!({
            "framework": "FCA AI Live Testing",
            "firm_user": event.user_id,
            "scenario": event.task,
            "supporting_refs": event.citations,
            "occurred_at": event.at_iso,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event() -> FinanceAuditEvent {
        FinanceAuditEvent {
            model_id: "anthropic/claude-opus".into(),
            user_id: "analyst-1".into(),
            task: "10-K research".into(),
            citations: vec!["10-K-2025#item7".into()],
            at_iso: "2026-04-22T09:00:00Z".into(),
        }
    }

    #[test]
    fn sr117_envelope_carries_framework_label() {
        let v = export_audit(&event(), AuditExportFormat::Sr117);
        assert_eq!(v["framework"], "SR 11-7");
        assert_eq!(v["evidence_refs"][0], "10-K-2025#item7");
    }

    #[test]
    fn feat_envelope_carries_framework_label() {
        let v = export_audit(&event(), AuditExportFormat::Feat);
        assert_eq!(v["framework"], "HKMA FEAT");
    }

    #[test]
    fn fria_envelope_carries_framework_label() {
        let v = export_audit(&event(), AuditExportFormat::EuFria);
        assert_eq!(v["framework"], "EU AI Act FRIA");
    }

    #[test]
    fn finra_envelope_carries_framework_label() {
        let v = export_audit(&event(), AuditExportFormat::Finra);
        assert_eq!(v["framework"], "FINRA Reg Notice 24-09");
    }

    #[test]
    fn fca_envelope_carries_framework_label() {
        let v = export_audit(&event(), AuditExportFormat::Fca);
        assert_eq!(v["framework"], "FCA AI Live Testing");
    }
}
