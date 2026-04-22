// IMPLEMENTS: D-284
//! Vision/audio/video per-turn sub-cap. GPT-5.4 with `detail=original`
//! costs roughly $0.10–$0.20 per turn for full-resolution screenshots —
//! left unbounded that drains the global cost cap fast. D-284 sets a
//! per-turn dollar limit and emits a `Degrade` event the planner can
//! react to (e.g. drop to half-resolution).

use serde::{Deserialize, Serialize};

pub const DEFAULT_VISION_TURN_CAP_USD: f64 = 0.20;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct VisionSubcap {
    pub turn_cap_usd: f64,
}

impl Default for VisionSubcap {
    fn default() -> Self {
        Self {
            turn_cap_usd: DEFAULT_VISION_TURN_CAP_USD,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DetailLevel {
    /// Half-resolution downsample — the degrade target.
    Low,
    /// Provider default.
    Auto,
    /// Full resolution.
    Original,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SubcapVerdict {
    Ok {
        used_usd: f64,
        cap_usd: f64,
    },
    Degrade {
        from: DetailLevel,
        to: DetailLevel,
        used_usd: f64,
        cap_usd: f64,
    },
    HardStop {
        used_usd: f64,
        cap_usd: f64,
    },
}

impl SubcapVerdict {
    #[must_use]
    pub fn is_degrade(&self) -> bool {
        matches!(self, Self::Degrade { .. })
    }
}

/// D-284: classify a per-turn vision charge. Above the cap → Degrade
/// (Original → Low) once; if the call returns a still-over-budget
/// estimate after the degrade, HardStop the rest of the turn.
#[must_use]
pub fn classify(cap: VisionSubcap, used_usd: f64, current_detail: DetailLevel) -> SubcapVerdict {
    if !used_usd.is_finite() || used_usd < 0.0 {
        return SubcapVerdict::Ok {
            used_usd: 0.0,
            cap_usd: cap.turn_cap_usd,
        };
    }
    if used_usd >= cap.turn_cap_usd {
        if matches!(current_detail, DetailLevel::Original) {
            return SubcapVerdict::Degrade {
                from: DetailLevel::Original,
                to: DetailLevel::Low,
                used_usd,
                cap_usd: cap.turn_cap_usd,
            };
        }
        return SubcapVerdict::HardStop {
            used_usd,
            cap_usd: cap.turn_cap_usd,
        };
    }
    SubcapVerdict::Ok {
        used_usd,
        cap_usd: cap.turn_cap_usd,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn under_cap_returns_ok() {
        let v = classify(VisionSubcap::default(), 0.05, DetailLevel::Original);
        assert!(matches!(v, SubcapVerdict::Ok { .. }));
    }

    #[test]
    fn over_cap_at_original_emits_degrade_to_low() {
        let v = classify(VisionSubcap::default(), 0.30, DetailLevel::Original);
        assert!(v.is_degrade());
        if let SubcapVerdict::Degrade { from, to, .. } = v {
            assert_eq!(from, DetailLevel::Original);
            assert_eq!(to, DetailLevel::Low);
        }
    }

    #[test]
    fn over_cap_at_low_hard_stops() {
        let v = classify(VisionSubcap::default(), 0.30, DetailLevel::Low);
        assert!(matches!(v, SubcapVerdict::HardStop { .. }));
    }

    #[test]
    fn negative_cost_is_ignored() {
        let v = classify(VisionSubcap::default(), -1.0, DetailLevel::Original);
        assert!(matches!(v, SubcapVerdict::Ok { .. }));
    }

    #[test]
    fn nan_cost_is_ignored() {
        let v = classify(VisionSubcap::default(), f64::NAN, DetailLevel::Original);
        assert!(matches!(v, SubcapVerdict::Ok { .. }));
    }
}
