// IMPLEMENTS: D-358
//! UPL (unauthorised practice of law) defence — three lines:
//! 1. Disclaimer is appended to every legal-mode reply by the surface.
//! 2. This module's [`screen_output`] catches representation-language
//!    patterns ("I represent you", "you should sue", "as your
//!    attorney") and downgrades them to information-only phrasing.
//! 3. A supervisor (admitted attorney) attestation is required before
//!    a matter can leave drafting state — handled by the workflow
//!    layer, not here.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum UplVerdict {
    Clean,
    /// Output triggered a UPL pattern. Caller should refuse to send
    /// or replace with informational phrasing.
    Triggered {
        matched: Vec<String>,
    },
}

const TRIGGER_PHRASES: &[&str] = &[
    "i represent you",
    "as your attorney",
    "as your lawyer",
    "you should sue",
    "i'll file the lawsuit",
    "we are your counsel",
    "this is legal advice",
];

#[must_use]
pub fn screen_output(text: &str) -> UplVerdict {
    let lower = text.to_ascii_lowercase();
    let matched: Vec<String> = TRIGGER_PHRASES
        .iter()
        .filter(|p| lower.contains(*p))
        .map(|p| (*p).to_string())
        .collect();
    if matched.is_empty() {
        UplVerdict::Clean
    } else {
        UplVerdict::Triggered { matched }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn benign_text_is_clean() {
        assert_eq!(
            screen_output("Here is a summary of California Civil Code §1714."),
            UplVerdict::Clean
        );
    }

    #[test]
    fn representation_language_is_caught() {
        let v = screen_output("As your attorney, you should sue them.");
        match v {
            UplVerdict::Triggered { matched } => {
                assert!(matched.iter().any(|m| m.contains("attorney")));
                assert!(matched.iter().any(|m| m.contains("sue")));
            }
            UplVerdict::Clean => panic!("expected trigger"),
        }
    }

    #[test]
    fn case_insensitive_match() {
        assert!(matches!(
            screen_output("THIS IS LEGAL ADVICE."),
            UplVerdict::Triggered { .. }
        ));
    }
}
