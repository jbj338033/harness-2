// IMPLEMENTS: D-375
//! AI washing guard. SEC's 2024 enforcement actions hinge on
//! marketing copy ("AI-driven", "powered by AI", "ML-managed") used
//! without substantive backing. This module flags a disclosure draft
//! when puffery appears without an adjacent capability statement.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AiWashingVerdict {
    Clean,
    /// Puffery phrase found without a substantive nearby phrase.
    Triggered {
        matched: Vec<String>,
    },
}

const PUFFERY: &[&str] = &[
    "ai-driven",
    "ai driven",
    "powered by ai",
    "ml-managed",
    "ai-managed",
    "fully autonomous",
    "next-generation ai",
];

const SUBSTANTIVE: &[&str] = &[
    "validated",
    "backtested",
    "human-reviewed",
    "audited by",
    "model card",
    "evaluation",
    "constraints",
];

#[must_use]
pub fn screen_disclosure(text: &str) -> AiWashingVerdict {
    let lower = text.to_ascii_lowercase();
    let matched: Vec<String> = PUFFERY
        .iter()
        .filter(|p| lower.contains(*p))
        .map(|p| (*p).to_string())
        .collect();
    if matched.is_empty() {
        return AiWashingVerdict::Clean;
    }
    let has_substantive = SUBSTANTIVE.iter().any(|p| lower.contains(p));
    if has_substantive {
        AiWashingVerdict::Clean
    } else {
        AiWashingVerdict::Triggered { matched }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pure_puffery_triggers() {
        let v = screen_disclosure("Our AI-driven, fully autonomous strategy.");
        match v {
            AiWashingVerdict::Triggered { matched } => assert!(!matched.is_empty()),
            AiWashingVerdict::Clean => panic!("expected trigger"),
        }
    }

    #[test]
    fn puffery_with_substantiation_passes() {
        let v =
            screen_disclosure("The AI-driven model is human-reviewed and backtested over 5 years.");
        assert_eq!(v, AiWashingVerdict::Clean);
    }

    #[test]
    fn benign_disclosure_clean() {
        let v = screen_disclosure("Quarterly performance update for institutional clients.");
        assert_eq!(v, AiWashingVerdict::Clean);
    }
}
