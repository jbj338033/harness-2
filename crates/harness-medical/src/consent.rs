// IMPLEMENTS: D-368
//! Consent documentation export — GDPR Art 9 (special-category data),
//! HIPAA 164.508 (authorisation), and 개보법 23조 (민감정보 동의).
//! The on-disk artefact is JSON; the surface formats it as a printable
//! consent record on demand.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LegalBasis {
    GdprArt9,
    Hipaa164_508,
    KrPipa23,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsentRecord {
    pub subject_id: String,
    pub purpose: String,
    pub bases: Vec<LegalBasis>,
    pub recorded_at_iso: String,
    pub revoked_at_iso: Option<String>,
}

pub const CONSENT_SCHEMA: &str = "harness/medical/consent/v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsentExport {
    pub schema: String,
    pub records: Vec<ConsentRecord>,
}

impl ConsentExport {
    #[must_use]
    pub fn new(records: Vec<ConsentRecord>) -> Self {
        Self {
            schema: CONSENT_SCHEMA.to_string(),
            records,
        }
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record() -> ConsentRecord {
        ConsentRecord {
            subject_id: "patient-1".into(),
            purpose: "AI-assisted summary of visit notes".into(),
            bases: vec![LegalBasis::GdprArt9, LegalBasis::Hipaa164_508],
            recorded_at_iso: "2026-04-22T10:00:00Z".into(),
            revoked_at_iso: None,
        }
    }

    #[test]
    fn export_round_trips_via_json() {
        let exp = ConsentExport::new(vec![record()]);
        let s = exp.to_json().unwrap();
        let back: ConsentExport = serde_json::from_str(&s).unwrap();
        assert_eq!(back.records.len(), 1);
        assert_eq!(back.schema, "harness/medical/consent/v1");
    }

    #[test]
    fn revoked_field_optional_and_round_trips() {
        let mut r = record();
        r.revoked_at_iso = Some("2026-05-01T00:00:00Z".into());
        let s = serde_json::to_string(&r).unwrap();
        let back: ConsentRecord = serde_json::from_str(&s).unwrap();
        assert_eq!(back.revoked_at_iso, Some("2026-05-01T00:00:00Z".into()));
    }

    #[test]
    fn three_bases_serialise_snake_case() {
        let r = ConsentRecord {
            bases: vec![
                LegalBasis::GdprArt9,
                LegalBasis::Hipaa164_508,
                LegalBasis::KrPipa23,
            ],
            ..record()
        };
        let s = serde_json::to_string(&r).unwrap();
        assert!(s.contains("gdpr_art9"));
        assert!(s.contains("hipaa164_508"));
        assert!(s.contains("kr_pipa23"));
    }
}
