// IMPLEMENTS: D-445
//! `model_registry.toml` row schema. Each row carries the eight
//! compliance fields needed for US BIS AI Diffusion + 한국 AI 기본법
//! cross-checks.

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BisTier {
    Tier1,
    Tier2,
    Tier3,
    Embargoed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistryRow {
    pub model_id: String,
    pub license: String,
    pub origin_country: String,
    pub bis_tier: BisTier,
    pub eu_art53_exempt: bool,
    pub kr_high_impact: bool,
    pub safety_profile: String,
    pub tool_format: String,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum RegistryRowError {
    #[error("registry row missing field: {0}")]
    Missing(&'static str),
    #[error("model is in BIS embargoed tier — refused")]
    BisEmbargoed,
}

pub fn validate_registry_row(row: &RegistryRow) -> Result<(), RegistryRowError> {
    if row.model_id.trim().is_empty() {
        return Err(RegistryRowError::Missing("model_id"));
    }
    if row.license.trim().is_empty() {
        return Err(RegistryRowError::Missing("license"));
    }
    if row.origin_country.trim().is_empty() {
        return Err(RegistryRowError::Missing("origin_country"));
    }
    if row.safety_profile.trim().is_empty() {
        return Err(RegistryRowError::Missing("safety_profile"));
    }
    if row.tool_format.trim().is_empty() {
        return Err(RegistryRowError::Missing("tool_format"));
    }
    if matches!(row.bis_tier, BisTier::Embargoed) {
        return Err(RegistryRowError::BisEmbargoed);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn full() -> RegistryRow {
        RegistryRow {
            model_id: "claude-opus".into(),
            license: "proprietary".into(),
            origin_country: "US".into(),
            bis_tier: BisTier::Tier1,
            eu_art53_exempt: false,
            kr_high_impact: false,
            safety_profile: "anthropic/asl-3".into(),
            tool_format: "tool_use".into(),
        }
    }

    #[test]
    fn full_row_validates() {
        assert!(validate_registry_row(&full()).is_ok());
    }

    #[test]
    fn missing_safety_profile_refused() {
        let mut r = full();
        r.safety_profile.clear();
        assert!(matches!(
            validate_registry_row(&r),
            Err(RegistryRowError::Missing("safety_profile"))
        ));
    }

    #[test]
    fn embargoed_tier_refused() {
        let mut r = full();
        r.bis_tier = BisTier::Embargoed;
        assert_eq!(
            validate_registry_row(&r),
            Err(RegistryRowError::BisEmbargoed)
        );
    }
}
