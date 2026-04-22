// IMPLEMENTS: D-440
//! Annual EU EED 2024/1364 Scope 3 cat.1 export. Enterprise procurement
//! treats Scope 3 cat.1 (purchased goods & services) pass-through as
//! a buying blocker — shipping a verifiable per-year report removes
//! that block.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Scope3Cat1Export {
    pub schema: String,
    pub reporting_year: u16,
    pub total_g_co2e: f64,
    pub by_provider: Vec<(String, f64)>,
    pub by_region: Vec<(String, f64)>,
    pub methodology_uri: String,
}

#[must_use]
pub fn build_scope3_cat1(
    reporting_year: u16,
    by_provider: &[(String, f64)],
    by_region: &[(String, f64)],
) -> Scope3Cat1Export {
    let total = by_provider
        .iter()
        .map(|(_, g)| *g)
        .filter(|g| g.is_finite())
        .sum();
    Scope3Cat1Export {
        schema: "harness/carbon/scope3-cat1/v1".into(),
        reporting_year,
        total_g_co2e: total,
        by_provider: by_provider.to_vec(),
        by_region: by_region.to_vec(),
        methodology_uri: "https://harness.local/methodology/carbon-v1".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn total_sums_provider_breakdown() {
        let e = build_scope3_cat1(
            2026,
            &[("anthropic".into(), 1_000.0), ("openai".into(), 500.0)],
            &[],
        );
        assert!((e.total_g_co2e - 1_500.0).abs() < 1e-6);
    }

    #[test]
    fn export_round_trips() {
        let e = build_scope3_cat1(
            2026,
            &[("anthropic".into(), 1_000.0)],
            &[("eu-west-1".into(), 1_000.0)],
        );
        let s = serde_json::to_string(&e).unwrap();
        let back: Scope3Cat1Export = serde_json::from_str(&s).unwrap();
        assert_eq!(back, e);
    }

    #[test]
    fn nan_provider_skipped_in_total() {
        let e = build_scope3_cat1(2026, &[("a".into(), 100.0), ("b".into(), f64::NAN)], &[]);
        assert!((e.total_g_co2e - 100.0).abs() < 1e-6);
    }
}
