// IMPLEMENTS: D-385
//! OMB M-25-22 — 8 procurement clauses must appear in the skill
//! frontmatter (use case, vendor, AI use type, model + version, data
//! sources, risk tier, monitoring plan, SBOM URI). Anything missing
//! blocks the skill from registering.

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OmbFrontmatter {
    pub use_case: String,
    pub vendor: String,
    pub ai_use_type: String,
    pub model_id: String,
    pub model_version: String,
    pub data_sources: Vec<String>,
    pub risk_tier: String,
    pub monitoring_plan_uri: String,
    /// CycloneDX SBOM pointer (URL or `sha256:...`).
    pub sbom_uri: String,
}

#[derive(Debug, Error)]
pub enum OmbValidationError {
    #[error("OMB M-25-22 missing field: {0}")]
    Missing(&'static str),
}

pub fn validate_omb_frontmatter(meta: &OmbFrontmatter) -> Result<(), OmbValidationError> {
    if meta.use_case.trim().is_empty() {
        return Err(OmbValidationError::Missing("use_case"));
    }
    if meta.vendor.trim().is_empty() {
        return Err(OmbValidationError::Missing("vendor"));
    }
    if meta.ai_use_type.trim().is_empty() {
        return Err(OmbValidationError::Missing("ai_use_type"));
    }
    if meta.model_id.trim().is_empty() {
        return Err(OmbValidationError::Missing("model_id"));
    }
    if meta.model_version.trim().is_empty() {
        return Err(OmbValidationError::Missing("model_version"));
    }
    if meta.data_sources.is_empty() {
        return Err(OmbValidationError::Missing("data_sources"));
    }
    if meta.risk_tier.trim().is_empty() {
        return Err(OmbValidationError::Missing("risk_tier"));
    }
    if meta.monitoring_plan_uri.trim().is_empty() {
        return Err(OmbValidationError::Missing("monitoring_plan_uri"));
    }
    if meta.sbom_uri.trim().is_empty() {
        return Err(OmbValidationError::Missing("sbom_uri"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn full() -> OmbFrontmatter {
        OmbFrontmatter {
            use_case: "benefits intake summary".into(),
            vendor: "Anthropic".into(),
            ai_use_type: "summarisation".into(),
            model_id: "claude-opus".into(),
            model_version: "4.7".into(),
            data_sources: vec!["application-form".into()],
            risk_tier: "high-impact".into(),
            monitoring_plan_uri: "https://example.gov/monitoring".into(),
            sbom_uri: "sha256:abc".into(),
        }
    }

    #[test]
    fn full_frontmatter_validates() {
        assert!(validate_omb_frontmatter(&full()).is_ok());
    }

    #[test]
    fn missing_sbom_rejected() {
        let mut f = full();
        f.sbom_uri.clear();
        assert!(matches!(
            validate_omb_frontmatter(&f),
            Err(OmbValidationError::Missing("sbom_uri"))
        ));
    }

    #[test]
    fn missing_data_sources_rejected() {
        let mut f = full();
        f.data_sources.clear();
        assert!(matches!(
            validate_omb_frontmatter(&f),
            Err(OmbValidationError::Missing("data_sources"))
        ));
    }
}
