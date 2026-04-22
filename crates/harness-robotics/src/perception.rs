// IMPLEMENTS: D-393
//! Dual-check perception with a confidence gate. UC Santa Cruz showed
//! environment-IPI (visual prompt injection through the workspace
//! itself) achieves ~64% attack success rate against single-stream
//! perception. The defence: require two independent perception
//! streams to agree above the confidence floor before the planner
//! treats their classification as ground truth.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PerceptionInput {
    pub label: String,
    pub confidence: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PerceptionVerdict {
    /// Both streams agree above floor.
    Agreed {
        label: String,
        confidence: f32,
    },
    Disagreement(PerceptionViolation),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PerceptionViolation {
    /// Streams returned different labels.
    LabelMismatch { primary: String, secondary: String },
    /// Either confidence below floor.
    BelowFloor {
        floor: f32,
        primary: f32,
        secondary: f32,
    },
}

#[must_use]
pub fn classify(
    primary: &PerceptionInput,
    secondary: &PerceptionInput,
    floor: f32,
) -> PerceptionVerdict {
    if primary.confidence < floor || secondary.confidence < floor {
        return PerceptionVerdict::Disagreement(PerceptionViolation::BelowFloor {
            floor,
            primary: primary.confidence,
            secondary: secondary.confidence,
        });
    }
    if primary.label != secondary.label {
        return PerceptionVerdict::Disagreement(PerceptionViolation::LabelMismatch {
            primary: primary.label.clone(),
            secondary: secondary.label.clone(),
        });
    }
    PerceptionVerdict::Agreed {
        label: primary.label.clone(),
        confidence: primary.confidence.min(secondary.confidence),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(label: &str, confidence: f32) -> PerceptionInput {
        PerceptionInput {
            label: label.into(),
            confidence,
        }
    }

    #[test]
    fn agreement_above_floor_passes() {
        let v = classify(&p("cup", 0.9), &p("cup", 0.95), 0.7);
        match v {
            PerceptionVerdict::Agreed { label, confidence } => {
                assert_eq!(label, "cup");
                assert!((confidence - 0.9).abs() < 1e-6);
            }
            _ => panic!("expected agreement"),
        }
    }

    #[test]
    fn label_mismatch_blocks() {
        let v = classify(&p("cup", 0.9), &p("mug", 0.9), 0.7);
        assert!(matches!(
            v,
            PerceptionVerdict::Disagreement(PerceptionViolation::LabelMismatch { .. })
        ));
    }

    #[test]
    fn below_floor_blocks() {
        let v = classify(&p("cup", 0.6), &p("cup", 0.9), 0.7);
        assert!(matches!(
            v,
            PerceptionVerdict::Disagreement(PerceptionViolation::BelowFloor { .. })
        ));
    }
}
