// IMPLEMENTS: D-444
//! Korean-provider preset descriptors. Locale must be `ko` AND
//! region must be `KR` for the preset to register. Cross-border
//! traffic is refused (한국 AI 기본법).

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KoreaPreset {
    SolarPro2,
    Kanana,
    HyperClovaXSeed,
    KtMidm,
}

#[must_use]
pub fn all_korea_presets() -> &'static [KoreaPreset] {
    use KoreaPreset::*;
    const ALL: &[KoreaPreset] = &[SolarPro2, Kanana, HyperClovaXSeed, KtMidm];
    ALL
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum KoreaGateError {
    #[error("korea preset requires locale=ko (got {0})")]
    LocaleMismatch(String),
    #[error("korea preset refuses region={0} — must be KR")]
    RegionMismatch(String),
}

pub fn gate_korea_locale(locale: &str, region: &str) -> Result<(), KoreaGateError> {
    if locale != "ko" {
        return Err(KoreaGateError::LocaleMismatch(locale.to_string()));
    }
    if region != "KR" {
        return Err(KoreaGateError::RegionMismatch(region.to_string()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn four_presets_listed() {
        assert_eq!(all_korea_presets().len(), 4);
    }

    #[test]
    fn ko_kr_passes_gate() {
        assert!(gate_korea_locale("ko", "KR").is_ok());
    }

    #[test]
    fn en_us_blocks() {
        assert!(matches!(
            gate_korea_locale("en", "US"),
            Err(KoreaGateError::LocaleMismatch(_))
        ));
    }

    #[test]
    fn ko_jp_blocks_on_region() {
        assert!(matches!(
            gate_korea_locale("ko", "JP"),
            Err(KoreaGateError::RegionMismatch(_))
        ));
    }
}
