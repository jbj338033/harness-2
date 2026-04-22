// IMPLEMENTS: D-381
//! Provenance over detection. We refuse to ship classifier-style "AI
//! detection" — Stanford's 2023/2025 work showed Turnitin and GPTZero
//! produce ~61% false-positive on ESL writing, which is
//! discriminatory by class composition.
//!
//! Instead, every learner-authored artefact carries a provenance
//! block: who edited which spans, when, and from which surface. This
//! gives an honest "human-AI mix" trail without classifying the text.

use serde::{Deserialize, Serialize};

/// Returned whenever a caller asks for AI detection. Always refused.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiDetectionRefusal {
    pub reason: &'static str,
    pub use_instead: &'static str,
}

#[must_use]
pub fn refuse_ai_detection() -> AiDetectionRefusal {
    AiDetectionRefusal {
        reason: "AI detection classifiers misclassify ESL writing at ~61% — refused for fairness",
        use_instead: "AuthorProvenance trail (who edited which spans + surface)",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthorRole {
    Learner,
    Tutor,
    AiAssistant,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthorSpan {
    pub byte_start: usize,
    pub byte_end: usize,
    pub author: AuthorRole,
    pub at_iso: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthorProvenance {
    pub document_id: String,
    pub spans: Vec<AuthorSpan>,
}

impl AuthorProvenance {
    #[must_use]
    pub fn ai_byte_count(&self) -> usize {
        self.spans
            .iter()
            .filter(|s| matches!(s.author, AuthorRole::AiAssistant))
            .map(|s| s.byte_end.saturating_sub(s.byte_start))
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detection_request_is_refused() {
        let r = refuse_ai_detection();
        assert!(r.reason.contains("ESL"));
        assert!(r.use_instead.contains("Provenance"));
    }

    #[test]
    fn ai_byte_count_sums_only_ai_spans() {
        let p = AuthorProvenance {
            document_id: "d1".into(),
            spans: vec![
                AuthorSpan {
                    byte_start: 0,
                    byte_end: 50,
                    author: AuthorRole::Learner,
                    at_iso: "t".into(),
                },
                AuthorSpan {
                    byte_start: 50,
                    byte_end: 80,
                    author: AuthorRole::AiAssistant,
                    at_iso: "t".into(),
                },
            ],
        };
        assert_eq!(p.ai_byte_count(), 30);
    }
}
