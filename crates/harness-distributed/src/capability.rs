// IMPLEMENTS: D-442
//! Runtime-measured ModelCapability. Replaces the older static tier
//! table — every recorded score must come from a real eval run, with
//! the corpus name attached so the consumer can sanity-check it.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityCorpus {
    SweBench,
    ToolCallSuite,
    StructuralBench,
    KoreanFamily,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MeasuredCapability {
    pub corpus: CapabilityCorpus,
    pub score_0_1: f32,
    pub measured_at_ms: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityTier {
    Trial,
    Standard,
    Professional,
}

#[must_use]
pub fn classify_capability(measurements: &[MeasuredCapability]) -> CapabilityTier {
    let avg = measurements
        .iter()
        .filter(|m| m.score_0_1.is_finite())
        .map(|m| f64::from(m.score_0_1))
        .sum::<f64>()
        / measurements
            .iter()
            .filter(|m| m.score_0_1.is_finite())
            .count()
            .max(1) as f64;
    if measurements.is_empty() {
        return CapabilityTier::Trial;
    }
    if avg >= 0.7 {
        CapabilityTier::Professional
    } else if avg >= 0.4 {
        CapabilityTier::Standard
    } else {
        CapabilityTier::Trial
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(c: CapabilityCorpus, s: f32) -> MeasuredCapability {
        MeasuredCapability {
            corpus: c,
            score_0_1: s,
            measured_at_ms: 1,
        }
    }

    #[test]
    fn empty_measurements_trial() {
        assert_eq!(classify_capability(&[]), CapabilityTier::Trial);
    }

    #[test]
    fn high_avg_professional() {
        assert_eq!(
            classify_capability(&[
                m(CapabilityCorpus::SweBench, 0.8),
                m(CapabilityCorpus::ToolCallSuite, 0.75)
            ]),
            CapabilityTier::Professional
        );
    }

    #[test]
    fn mid_avg_standard() {
        assert_eq!(
            classify_capability(&[m(CapabilityCorpus::StructuralBench, 0.5)]),
            CapabilityTier::Standard
        );
    }
}
