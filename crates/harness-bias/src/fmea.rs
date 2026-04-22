// IMPLEMENTS: D-307
//! FMEA release gate with a bias axis. Each entry is scored on
//! Severity × Occurrence × Detection × Bias (1–10 each); the RPN
//! product must stay under [`RPN_GATE`] for the release to ship.

use serde::{Deserialize, Serialize};

pub const RPN_GATE: u32 = 200;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FmeaEntry {
    pub failure_mode: String,
    pub severity: u8,
    pub occurrence: u8,
    pub detection: u8,
    pub bias_axis: u8,
}

impl FmeaEntry {
    #[must_use]
    pub fn rpn(&self) -> u32 {
        u32::from(self.severity)
            * u32::from(self.occurrence)
            * u32::from(self.detection)
            * u32::from(self.bias_axis)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FmeaVerdict {
    Pass,
    /// Listed in descending RPN.
    Fail(Vec<FmeaEntry>),
}

#[must_use]
pub fn evaluate_fmea(entries: &[FmeaEntry]) -> FmeaVerdict {
    let mut failures: Vec<FmeaEntry> = entries
        .iter()
        .filter(|e| e.rpn() > RPN_GATE)
        .cloned()
        .collect();
    if failures.is_empty() {
        return FmeaVerdict::Pass;
    }
    failures.sort_by_key(|e| std::cmp::Reverse(e.rpn()));
    FmeaVerdict::Fail(failures)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(s: u8, o: u8, d: u8, b: u8) -> FmeaEntry {
        FmeaEntry {
            failure_mode: format!("mode-{s}{o}{d}{b}"),
            severity: s,
            occurrence: o,
            detection: d,
            bias_axis: b,
        }
    }

    #[test]
    fn under_gate_passes() {
        let v = evaluate_fmea(&[entry(2, 3, 4, 2)]);
        assert!(matches!(v, FmeaVerdict::Pass));
    }

    #[test]
    fn over_gate_returns_descending() {
        let v = evaluate_fmea(&[entry(8, 8, 4, 2), entry(8, 8, 4, 5), entry(2, 2, 2, 2)]);
        match v {
            FmeaVerdict::Fail(f) => {
                assert_eq!(f.len(), 2);
                assert!(f[0].rpn() >= f[1].rpn());
            }
            FmeaVerdict::Pass => panic!("expected fail"),
        }
    }

    #[test]
    fn rpn_multiplies_four_axes() {
        let e = entry(2, 3, 4, 5);
        assert_eq!(e.rpn(), 120);
    }
}
