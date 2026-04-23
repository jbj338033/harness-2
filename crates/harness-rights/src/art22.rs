// IMPLEMENTS: D-185
//! GDPR Art 22 right-to-explanation export. The data subject can
//! request "meaningful information about the logic involved" for any
//! solely-automated decision affecting them. We expose that as an
//! envelope that names the model, the inputs that mattered, and the
//! human review path.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Art22ExplanationExport {
    pub schema: String,
    pub data_subject_id: String,
    pub decision_at_iso: String,
    pub model_id: String,
    pub principal_factors: Vec<String>,
    pub human_review_path: String,
    pub article_citation: String,
}

pub const ARTICLE_CITATION: &str = "GDPR 2016/679 Art 22 + Recital 71";

#[must_use]
pub fn build_art22_explanation(
    data_subject_id: impl Into<String>,
    decision_at_iso: impl Into<String>,
    model_id: impl Into<String>,
    principal_factors: Vec<String>,
    human_review_path: impl Into<String>,
) -> Art22ExplanationExport {
    Art22ExplanationExport {
        schema: "harness/rights/art22/v1".into(),
        data_subject_id: data_subject_id.into(),
        decision_at_iso: decision_at_iso.into(),
        model_id: model_id.into(),
        principal_factors,
        human_review_path: human_review_path.into(),
        article_citation: ARTICLE_CITATION.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_carries_article_citation() {
        let e = build_art22_explanation(
            "ds-1",
            "2026-04-22",
            "claude-opus",
            vec!["income".into()],
            "ombudsman@example.com",
        );
        assert!(e.article_citation.contains("Art 22"));
        assert_eq!(e.principal_factors, vec!["income"]);
    }

    #[test]
    fn round_trips_via_serde() {
        let e = build_art22_explanation("ds-1", "2026-04-22", "m", vec!["a".into()], "p");
        let s = serde_json::to_string(&e).unwrap();
        let back: Art22ExplanationExport = serde_json::from_str(&s).unwrap();
        assert_eq!(back, e);
    }
}
