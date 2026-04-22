// IMPLEMENTS: D-312
//! Post-hoc Explanation Projection. Combines a natural-language
//! summary (D-265 semantic adapter) with the structured JSON record
//! (D-271 recipe display + raw event payloads). Output URI is
//! Agent-Trace–compatible — every reconstruction has the same
//! `agent-trace://` shape so a third-party trace viewer can follow.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PostHocSource {
    pub turn_id: String,
    pub natural_language: String,
    pub structured: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExplanationProjection {
    pub trace_uri: String,
    pub natural_language: String,
    pub structured: serde_json::Value,
    pub digest: String,
}

#[must_use]
pub fn build_explanation(src: PostHocSource) -> ExplanationProjection {
    let mut h = blake3::Hasher::new();
    h.update(b"harness/post-hoc/v1\n");
    h.update(src.turn_id.as_bytes());
    h.update(src.natural_language.as_bytes());
    let bytes = serde_json::to_vec(&src.structured).unwrap_or_default();
    h.update(&bytes);
    let digest_full = h.finalize().to_hex();
    let short: String = digest_full.chars().take(16).collect();
    ExplanationProjection {
        trace_uri: format!("agent-trace://turn/{}", src.turn_id),
        natural_language: src.natural_language,
        structured: src.structured,
        digest: short,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projection_uri_is_agent_trace() {
        let p = build_explanation(PostHocSource {
            turn_id: "t-1".into(),
            natural_language: "tested with N=3".into(),
            structured: serde_json::json!({"n": 3}),
        });
        assert!(p.trace_uri.starts_with("agent-trace://turn/"));
        assert_eq!(p.digest.len(), 16);
    }

    #[test]
    fn digest_is_deterministic() {
        let s = || PostHocSource {
            turn_id: "t-1".into(),
            natural_language: "x".into(),
            structured: serde_json::json!({"a": 1}),
        };
        assert_eq!(build_explanation(s()).digest, build_explanation(s()).digest);
    }
}
