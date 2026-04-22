// IMPLEMENTS: D-367
//! SaMD (Software as a Medical Device) boundary. Skill presets that
//! drift into Class II territory — diagnosis, treatment recommendation,
//! triage decision — are refused. We pattern-match the preset's
//! declared `intent` field; the presets crate forwards them here.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SamdRefusal {
    Allow,
    /// The preset reads as Class II SaMD — refuse with the reason.
    RefuseSamd {
        matched_intent: String,
    },
}

const FORBIDDEN_INTENTS: &[&str] = &[
    "diagnose",
    "diagnosis",
    "triage",
    "treatment plan",
    "dose adjustment",
    "prescribe",
    "interpret radiology",
];

#[must_use]
pub fn classify_skill_preset(intent: &str) -> SamdRefusal {
    let lower = intent.to_ascii_lowercase();
    if let Some(matched) = FORBIDDEN_INTENTS.iter().find(|p| lower.contains(*p)) {
        SamdRefusal::RefuseSamd {
            matched_intent: (*matched).to_string(),
        }
    } else {
        SamdRefusal::Allow
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn note_taking_preset_allowed() {
        assert_eq!(
            classify_skill_preset("structured visit note from clinician dictation"),
            SamdRefusal::Allow
        );
    }

    #[test]
    fn diagnosis_preset_refused() {
        match classify_skill_preset("Diagnose chest pain in adult patient") {
            SamdRefusal::RefuseSamd { matched_intent } => assert_eq!(matched_intent, "diagnose"),
            SamdRefusal::Allow => panic!("expected refusal"),
        }
    }

    #[test]
    fn radiology_interpretation_refused() {
        assert!(matches!(
            classify_skill_preset("Interpret radiology film for findings"),
            SamdRefusal::RefuseSamd { .. }
        ));
    }
}
