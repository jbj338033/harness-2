// IMPLEMENTS: D-386
//! Unified AIA (Algorithmic Impact Assessment) export. Three target
//! schemas: Canada AIA Directive, EU AI Act FRIA, Colorado SB24-205.
//! All three reduce to the same source `AiaExport`; this module wraps
//! it in each schema's envelope.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiaFormat {
    CanadaAia,
    EuFria,
    ColoradoSb24_205,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AiaExport {
    pub system_id: String,
    pub deploying_agency: String,
    pub purpose: String,
    pub impact_level: u8,
    pub safeguards: Vec<String>,
    pub recorded_at_iso: String,
}

#[must_use]
pub fn export_aia(source: &AiaExport, format: AiaFormat) -> serde_json::Value {
    match format {
        AiaFormat::CanadaAia => serde_json::json!({
            "directive": "Canada AIA",
            "system": source.system_id,
            "agency": source.deploying_agency,
            "purpose": source.purpose,
            "impact_level": source.impact_level,
            "mitigations": source.safeguards,
            "completed_at": source.recorded_at_iso,
        }),
        AiaFormat::EuFria => serde_json::json!({
            "framework": "EU AI Act FRIA",
            "system": source.system_id,
            "deployer": source.deploying_agency,
            "purpose": source.purpose,
            "rights_impact_tier": source.impact_level,
            "safeguards": source.safeguards,
            "timestamp": source.recorded_at_iso,
        }),
        AiaFormat::ColoradoSb24_205 => serde_json::json!({
            "law": "Colorado SB24-205",
            "system": source.system_id,
            "deployer": source.deploying_agency,
            "consequential_decision": source.purpose,
            "impact_level": source.impact_level,
            "safeguards": source.safeguards,
            "completed_at": source.recorded_at_iso,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn src() -> AiaExport {
        AiaExport {
            system_id: "benefits-summariser-v1".into(),
            deploying_agency: "DHS".into(),
            purpose: "intake summary".into(),
            impact_level: 3,
            safeguards: vec!["human-review".into()],
            recorded_at_iso: "2026-04-22".into(),
        }
    }

    #[test]
    fn canada_envelope() {
        let v = export_aia(&src(), AiaFormat::CanadaAia);
        assert_eq!(v["directive"], "Canada AIA");
    }

    #[test]
    fn fria_envelope() {
        let v = export_aia(&src(), AiaFormat::EuFria);
        assert_eq!(v["framework"], "EU AI Act FRIA");
    }

    #[test]
    fn sb24_205_envelope() {
        let v = export_aia(&src(), AiaFormat::ColoradoSb24_205);
        assert_eq!(v["law"], "Colorado SB24-205");
    }
}
