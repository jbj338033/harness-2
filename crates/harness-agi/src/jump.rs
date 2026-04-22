// IMPLEMENTS: D-431
//! Capability Jump Detector. Compares the latest benchmarked vector
//! against the prior baseline. Any axis that jumps by more than the
//! configured `delta` (default 0.10 absolute) marks the model
//! "needs re-fetched capability card" — D-341 was static at startup,
//! this brings it dynamic.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CapabilityVector {
    pub general: f32,
    pub agentic: f32,
    pub coding: f32,
    pub reasoning: f32,
    pub safety_eval: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum JumpVerdict {
    NoJump,
    /// One or more axes jumped — list them with delta.
    Jumped(Vec<(String, f32)>),
}

const DEFAULT_DELTA: f32 = 0.10;

#[must_use]
pub fn classify_jump(prev: CapabilityVector, latest: CapabilityVector) -> JumpVerdict {
    let mut bumps: Vec<(String, f32)> = Vec::new();
    for (label, p, l) in [
        ("general", prev.general, latest.general),
        ("agentic", prev.agentic, latest.agentic),
        ("coding", prev.coding, latest.coding),
        ("reasoning", prev.reasoning, latest.reasoning),
        ("safety_eval", prev.safety_eval, latest.safety_eval),
    ] {
        let d = l - p;
        if d.is_finite() && d > DEFAULT_DELTA {
            bumps.push((label.to_string(), d));
        }
    }
    if bumps.is_empty() {
        JumpVerdict::NoJump
    } else {
        JumpVerdict::Jumped(bumps)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cv(g: f32, a: f32, c: f32, r: f32, s: f32) -> CapabilityVector {
        CapabilityVector {
            general: g,
            agentic: a,
            coding: c,
            reasoning: r,
            safety_eval: s,
        }
    }

    #[test]
    fn no_jump_when_within_delta() {
        let v = classify_jump(cv(0.7, 0.6, 0.7, 0.7, 0.8), cv(0.78, 0.61, 0.74, 0.7, 0.8));
        assert!(matches!(v, JumpVerdict::NoJump));
    }

    #[test]
    fn jump_on_agentic_axis_caught() {
        let v = classify_jump(cv(0.7, 0.5, 0.7, 0.7, 0.8), cv(0.7, 0.7, 0.7, 0.7, 0.8));
        match v {
            JumpVerdict::Jumped(b) => {
                assert!(b.iter().any(|(label, d)| label == "agentic" && *d > 0.10));
            }
            JumpVerdict::NoJump => panic!("expected jump"),
        }
    }

    #[test]
    fn negative_delta_does_not_count() {
        let v = classify_jump(cv(0.9, 0.9, 0.9, 0.9, 0.9), cv(0.5, 0.5, 0.5, 0.5, 0.5));
        assert!(matches!(v, JumpVerdict::NoJump));
    }
}
