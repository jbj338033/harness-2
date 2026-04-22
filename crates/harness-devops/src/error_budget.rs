// IMPLEMENTS: D-257
//! Error Budget Policy (sibling to D-150 cost cap). When the SLO
//! burn-rate crosses the policy bands, return the resulting verdict
//! so the orchestrator can throttle deploys or trigger an incident.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ErrorBudgetPolicy {
    /// SLO target (eg. 0.999 for three-9s).
    pub slo_target: f64,
    /// Window in hours used by the burn-rate calc.
    pub window_hours: f32,
    pub warn_burn_rate: f32,
    pub page_burn_rate: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BurnRateVerdict {
    Healthy,
    Warn,
    Page,
}

#[must_use]
pub fn evaluate_burn_rate(policy: ErrorBudgetPolicy, observed_burn_rate: f32) -> BurnRateVerdict {
    if !observed_burn_rate.is_finite() || observed_burn_rate < 0.0 {
        return BurnRateVerdict::Healthy;
    }
    if observed_burn_rate >= policy.page_burn_rate {
        BurnRateVerdict::Page
    } else if observed_burn_rate >= policy.warn_burn_rate {
        BurnRateVerdict::Warn
    } else {
        BurnRateVerdict::Healthy
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy() -> ErrorBudgetPolicy {
        ErrorBudgetPolicy {
            slo_target: 0.999,
            window_hours: 1.0,
            warn_burn_rate: 2.0,
            page_burn_rate: 14.4,
        }
    }

    #[test]
    fn under_warn_is_healthy() {
        assert_eq!(evaluate_burn_rate(policy(), 1.0), BurnRateVerdict::Healthy);
    }

    #[test]
    fn between_warn_and_page() {
        assert_eq!(evaluate_burn_rate(policy(), 5.0), BurnRateVerdict::Warn);
    }

    #[test]
    fn above_page_threshold() {
        assert_eq!(evaluate_burn_rate(policy(), 20.0), BurnRateVerdict::Page);
    }

    #[test]
    fn nan_burn_treated_as_healthy() {
        assert_eq!(
            evaluate_burn_rate(policy(), f32::NAN),
            BurnRateVerdict::Healthy
        );
    }
}
