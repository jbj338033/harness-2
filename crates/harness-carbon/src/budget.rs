// IMPLEMENTS: D-439
//! Carbon-axis budget cap. Sibling to D-150 cost cap. The verdict
//! tier mirrors the existing 3-band (Ok / SoftWarn / HardStop) so the
//! TUI can reuse the same banner.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CarbonBudget {
    pub session_cap_g_co2e: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CarbonBudgetVerdict {
    Ok,
    SoftWarn,
    HardStop,
}

#[must_use]
pub fn classify_carbon(budget: CarbonBudget, used_g_co2e: f64) -> CarbonBudgetVerdict {
    if !used_g_co2e.is_finite() || used_g_co2e < 0.0 {
        return CarbonBudgetVerdict::Ok;
    }
    if used_g_co2e >= budget.session_cap_g_co2e {
        return CarbonBudgetVerdict::HardStop;
    }
    if used_g_co2e >= budget.session_cap_g_co2e * 0.9 {
        return CarbonBudgetVerdict::SoftWarn;
    }
    CarbonBudgetVerdict::Ok
}

#[cfg(test)]
mod tests {
    use super::*;

    fn budget() -> CarbonBudget {
        CarbonBudget {
            session_cap_g_co2e: 100.0,
        }
    }

    #[test]
    fn under_band_ok() {
        assert_eq!(classify_carbon(budget(), 10.0), CarbonBudgetVerdict::Ok);
    }

    #[test]
    fn at_ninety_warn() {
        assert_eq!(
            classify_carbon(budget(), 90.0),
            CarbonBudgetVerdict::SoftWarn
        );
    }

    #[test]
    fn over_cap_hardstop() {
        assert_eq!(
            classify_carbon(budget(), 105.0),
            CarbonBudgetVerdict::HardStop
        );
    }

    #[test]
    fn negative_treated_as_ok() {
        assert_eq!(classify_carbon(budget(), -5.0), CarbonBudgetVerdict::Ok);
    }
}
