// IMPLEMENTS: D-388
//! Rights-impacting false-positive cap + cohort audit + due-process
//! pattern detection. The Michigan MiDAS unemployment-fraud system
//! (~93% false-positive rate, $20M settlement) is the canonical
//! cautionary tale — we refuse to ship a model that exceeds the cap
//! on any cohort.

use serde::{Deserialize, Serialize};

/// 5% — chosen because it's an order of magnitude below MiDAS and
/// matches the bias-audit ceiling shared with R41 D-300.
pub const FALSE_POSITIVE_CAP: f64 = 0.05;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RightsCohort {
    pub label: String,
    pub population: u64,
    pub false_positive_rate: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DueProcessVerdict {
    Pass,
    /// At least one cohort exceeded [`FALSE_POSITIVE_CAP`]. Listed in
    /// descending order of breach severity so the surface can show
    /// the worst first.
    BreachedCohorts(Vec<RightsCohort>),
}

#[must_use]
pub fn evaluate_due_process(cohorts: &[RightsCohort]) -> DueProcessVerdict {
    let mut breached: Vec<RightsCohort> = cohorts
        .iter()
        .filter(|c| c.false_positive_rate.is_finite() && c.false_positive_rate > FALSE_POSITIVE_CAP)
        .cloned()
        .collect();
    if breached.is_empty() {
        return DueProcessVerdict::Pass;
    }
    breached.sort_by(|a, b| {
        b.false_positive_rate
            .partial_cmp(&a.false_positive_rate)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    DueProcessVerdict::BreachedCohorts(breached)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cohort(label: &str, fp: f64) -> RightsCohort {
        RightsCohort {
            label: label.into(),
            population: 1000,
            false_positive_rate: fp,
        }
    }

    #[test]
    fn under_cap_passes() {
        let v = evaluate_due_process(&[cohort("A", 0.01), cohort("B", 0.04)]);
        assert!(matches!(v, DueProcessVerdict::Pass));
    }

    #[test]
    fn breach_returns_descending_severity() {
        let v = evaluate_due_process(&[cohort("A", 0.06), cohort("B", 0.20), cohort("C", 0.02)]);
        match v {
            DueProcessVerdict::BreachedCohorts(c) => {
                assert_eq!(c.len(), 2);
                assert_eq!(c[0].label, "B");
                assert_eq!(c[1].label, "A");
            }
            DueProcessVerdict::Pass => panic!("expected breach"),
        }
    }

    #[test]
    fn nan_rate_does_not_breach() {
        let v = evaluate_due_process(&[cohort("A", f64::NAN)]);
        assert!(matches!(v, DueProcessVerdict::Pass));
    }
}
