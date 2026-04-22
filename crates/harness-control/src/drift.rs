// IMPLEMENTS: D-311
//! Intent Drift Check — long-running workers self-check at the
//! lesser of 20 turns OR 30 minutes since the last confirmation.
//! "Still on the original goal?" — if the worker is no longer
//! aligned, the result is `MustReconfirm`.

use serde::{Deserialize, Serialize};

const DRIFT_TURN_BUDGET: u32 = 20;
const DRIFT_TIME_BUDGET_MS: i64 = 30 * 60 * 1000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DriftCheck {
    pub turns_since_confirm: u32,
    pub ms_since_confirm: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DriftCheckOutcome {
    /// No check needed yet.
    InsideBudget,
    /// Turn budget exceeded.
    MustReconfirmBecauseTurns,
    /// Time budget exceeded.
    MustReconfirmBecauseTime,
}

#[must_use]
pub fn due_drift_check(check: &DriftCheck) -> DriftCheckOutcome {
    if check.turns_since_confirm >= DRIFT_TURN_BUDGET {
        return DriftCheckOutcome::MustReconfirmBecauseTurns;
    }
    if check.ms_since_confirm >= DRIFT_TIME_BUDGET_MS {
        return DriftCheckOutcome::MustReconfirmBecauseTime;
    }
    DriftCheckOutcome::InsideBudget
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_worker_inside_budget() {
        assert_eq!(
            due_drift_check(&DriftCheck {
                turns_since_confirm: 5,
                ms_since_confirm: 60_000,
            }),
            DriftCheckOutcome::InsideBudget
        );
    }

    #[test]
    fn twenty_turns_triggers() {
        assert_eq!(
            due_drift_check(&DriftCheck {
                turns_since_confirm: 20,
                ms_since_confirm: 0,
            }),
            DriftCheckOutcome::MustReconfirmBecauseTurns
        );
    }

    #[test]
    fn thirty_minutes_triggers() {
        assert_eq!(
            due_drift_check(&DriftCheck {
                turns_since_confirm: 0,
                ms_since_confirm: DRIFT_TIME_BUDGET_MS,
            }),
            DriftCheckOutcome::MustReconfirmBecauseTime
        );
    }
}
