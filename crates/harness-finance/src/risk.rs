// IMPLEMENTS: D-373
//! Per-user VaR / volatility constraint memory rows. The memory store
//! holds these as typed JSON; this crate is the schema.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskMetric {
    Var95,
    Var99,
    AnnualisedVolatility,
    MaxDrawdown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RiskConstraintRow {
    pub user_id: String,
    pub metric: RiskMetric,
    /// Upper bound (e.g. 0.20 = 20%). Backtest plans whose simulated
    /// metric exceeds this bound must surface a warning before the
    /// reply is shown.
    pub limit: f64,
    pub recorded_at_iso: String,
}

impl RiskConstraintRow {
    #[must_use]
    pub fn breaches(&self, observed: f64) -> bool {
        observed.is_finite() && observed > self.limit
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(metric: RiskMetric, limit: f64) -> RiskConstraintRow {
        RiskConstraintRow {
            user_id: "u1".into(),
            metric,
            limit,
            recorded_at_iso: "2026-04-22".into(),
        }
    }

    #[test]
    fn breach_when_observed_exceeds_limit() {
        let r = row(RiskMetric::Var95, 0.05);
        assert!(r.breaches(0.07));
        assert!(!r.breaches(0.04));
    }

    #[test]
    fn nan_does_not_breach() {
        let r = row(RiskMetric::Var99, 0.10);
        assert!(!r.breaches(f64::NAN));
    }

    #[test]
    fn schema_round_trips() {
        let r = row(RiskMetric::AnnualisedVolatility, 0.25);
        let s = serde_json::to_string(&r).unwrap();
        let back: RiskConstraintRow = serde_json::from_str(&s).unwrap();
        assert_eq!(back, r);
    }
}
