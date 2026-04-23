// IMPLEMENTS: D-213
//! 1000-user CI workload simulation. The plan declares the user
//! count and the per-user think-time / turn count; the result
//! reports observed p50/p95 turn latency and the failure rate so the
//! gate can compare against the SLO.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CiSimulationPlan {
    pub user_count: u32,
    pub turns_per_user: u32,
    pub think_time_ms: u32,
}

impl CiSimulationPlan {
    #[must_use]
    pub fn default_thousand_user() -> Self {
        Self {
            user_count: 1000,
            turns_per_user: 8,
            think_time_ms: 750,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CiSimulationResult {
    pub p50_turn_ms: u32,
    pub p95_turn_ms: u32,
    pub failure_rate: f32,
    /// Slo: p50 ≤ 2 s, p95 ≤ 5 s, failure ≤ 1 %.
    pub passed: bool,
}

const P50_SLO_MS: u32 = 2_000;
const P95_SLO_MS: u32 = 5_000;
const FAILURE_RATE_SLO: f32 = 0.01;

#[must_use]
pub fn evaluate_simulation(
    p50_turn_ms: u32,
    p95_turn_ms: u32,
    failure_rate: f32,
) -> CiSimulationResult {
    let passed = failure_rate.is_finite()
        && failure_rate <= FAILURE_RATE_SLO
        && p50_turn_ms <= P50_SLO_MS
        && p95_turn_ms <= P95_SLO_MS;
    CiSimulationResult {
        p50_turn_ms,
        p95_turn_ms,
        failure_rate,
        passed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_plan_targets_thousand_users() {
        assert_eq!(CiSimulationPlan::default_thousand_user().user_count, 1000);
    }

    #[test]
    fn under_slo_passes() {
        let r = evaluate_simulation(1_500, 4_000, 0.005);
        assert!(r.passed);
    }

    #[test]
    fn over_p95_fails() {
        let r = evaluate_simulation(1_500, 6_000, 0.005);
        assert!(!r.passed);
    }

    #[test]
    fn over_failure_rate_fails() {
        let r = evaluate_simulation(1_500, 4_000, 0.05);
        assert!(!r.passed);
    }

    #[test]
    fn nan_failure_rate_fails() {
        let r = evaluate_simulation(1_500, 4_000, f32::NAN);
        assert!(!r.passed);
    }
}
