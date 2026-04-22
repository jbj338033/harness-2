// IMPLEMENTS: D-400
//! AADC (Age Appropriate Design Code) 7-pattern CI gate. Any release
//! that ships UI copy or flow matching one of these patterns fails
//! the gate. We deliberately keep this rule-based — the whole point
//! of an AADC scan is that it's auditable and not learned.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DarkPatternKind {
    Confirmshaming,
    PreSelectedConsent,
    NaggingNotifications,
    ForcedAction,
    PrivacyZuckering,
    EngagementMaximisation,
    DisguisedAds,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DarkPatternFinding {
    pub kind: DarkPatternKind,
    pub matched_phrase: String,
}

const RULES: &[(DarkPatternKind, &[&str])] = &[
    (
        DarkPatternKind::Confirmshaming,
        &["no thanks, i hate", "no, i don't care about"],
    ),
    (
        DarkPatternKind::PreSelectedConsent,
        &["checked by default", "opt-out by default"],
    ),
    (
        DarkPatternKind::NaggingNotifications,
        &["are you sure you want to leave", "we'll keep reminding"],
    ),
    (
        DarkPatternKind::ForcedAction,
        &["you must accept", "continue to use this app you must"],
    ),
    (
        DarkPatternKind::PrivacyZuckering,
        &[
            "share with friends to continue",
            "import your contacts to unlock",
        ],
    ),
    (
        DarkPatternKind::EngagementMaximisation,
        &["streak broken", "don't lose your streak"],
    ),
    (
        DarkPatternKind::DisguisedAds,
        &["sponsored result", "promoted suggestion"],
    ),
];

#[must_use]
pub fn scan_for_aadc_violations(text: &str) -> Vec<DarkPatternFinding> {
    let lower = text.to_ascii_lowercase();
    let mut out = Vec::new();
    for (kind, phrases) in RULES {
        for phrase in *phrases {
            if lower.contains(phrase) {
                out.push(DarkPatternFinding {
                    kind: *kind,
                    matched_phrase: (*phrase).to_string(),
                });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn benign_copy_clean() {
        assert!(scan_for_aadc_violations("Welcome to the dashboard.").is_empty());
    }

    #[test]
    fn confirmshaming_caught() {
        let f = scan_for_aadc_violations("No thanks, I hate saving money");
        assert!(
            f.iter()
                .any(|x| matches!(x.kind, DarkPatternKind::Confirmshaming))
        );
    }

    #[test]
    fn pre_selected_consent_caught() {
        let f = scan_for_aadc_violations("Marketing emails: checked by default");
        assert!(
            f.iter()
                .any(|x| matches!(x.kind, DarkPatternKind::PreSelectedConsent))
        );
    }

    #[test]
    fn engagement_maximisation_caught() {
        let f = scan_for_aadc_violations("Don't lose your streak — log in tonight!");
        assert!(
            f.iter()
                .any(|x| matches!(x.kind, DarkPatternKind::EngagementMaximisation))
        );
    }
}
