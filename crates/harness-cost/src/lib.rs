// IMPLEMENTS: D-150, D-162, D-166, D-174, D-195, D-239
//! Three-tier cost cap. Defaults are $5 / session, $50 / day, $500 / global
//! per D-150. D-162 adds a $2 safety margin: at 90% of any cap we soft-warn,
//! at 100% (margin-adjusted) we hard-stop. Multi-session aggregation rolls
//! up via the writer-actor (D-174) so caller code never reads partial state.
//!
//! See [`billing`] for the Harness-estimate ↔ provider-confirm reconciler
//! that resolves the cost split-brain (D-195, D-239), [`abort_marker`] for
//! the atomic ledger-update + abort coupling (D-174), and [`repeat_detector`]
//! for the same-action detector that resets on cost-cap resume (D-166).

pub mod abort_marker;
pub mod billing;
pub mod repeat_detector;

pub use abort_marker::{AbortMarker, AbortReason, UpdateOutcome, apply_charge_with_marker};
pub use billing::{
    AuthoritySource, DEFAULT_POLL_INTERVAL, ESTIMATE_EARLY_WARN_RATIO, ReconcileLedger,
    ReconcileVerdict,
};
pub use repeat_detector::{RepeatDetector, RepeatVerdict};

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const DEFAULT_SESSION_CAP_USD: f64 = 5.0;
pub const DEFAULT_DAILY_CAP_USD: f64 = 50.0;
pub const DEFAULT_GLOBAL_CAP_USD: f64 = 500.0;
pub const DEFAULT_SAFETY_MARGIN_USD: f64 = 2.0;
pub const SOFT_WARN_RATIO: f64 = 0.90;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CostError {
    #[error("negative cost {0} is not allowed")]
    Negative(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CostCap {
    pub session_usd: f64,
    pub daily_usd: f64,
    pub global_usd: f64,
    /// D-162a: subtract this much from each effective cap so provider
    /// billing drift can't quietly overshoot.
    pub safety_margin_usd: f64,
}

impl Default for CostCap {
    fn default() -> Self {
        Self {
            session_usd: DEFAULT_SESSION_CAP_USD,
            daily_usd: DEFAULT_DAILY_CAP_USD,
            global_usd: DEFAULT_GLOBAL_CAP_USD,
            safety_margin_usd: DEFAULT_SAFETY_MARGIN_USD,
        }
    }
}

impl CostCap {
    #[must_use]
    pub fn effective(&self, tier: Tier) -> f64 {
        let raw = match tier {
            Tier::Session => self.session_usd,
            Tier::Daily => self.daily_usd,
            Tier::Global => self.global_usd,
        };
        (raw - self.safety_margin_usd).max(0.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tier {
    Session,
    Daily,
    Global,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CostStatus {
    Ok,
    SoftWarn { tier: Tier, used: f64, cap: f64 },
    HardStop { tier: Tier, used: f64, cap: f64 },
}

impl CostStatus {
    #[must_use]
    pub fn is_hard_stop(&self) -> bool {
        matches!(self, Self::HardStop { .. })
    }
}

/// Walking sums per tier. The writer-actor (D-174) is the only path that
/// mutates these; readers see a consistent triple.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
pub struct CostLedger {
    pub session_used: f64,
    pub daily_used: f64,
    pub global_used: f64,
}

impl CostLedger {
    /// Add a new charge to all three tiers. Returns the post-update status
    /// against `cap` so the caller can decide whether to abort (D-162e).
    ///
    /// # Errors
    /// Refuses negative charges — that would let a faulty provider quietly
    /// reset the ledger past its safety margin.
    pub fn record(&mut self, cap: &CostCap, charge_usd: f64) -> Result<CostStatus, CostError> {
        if !charge_usd.is_finite() || charge_usd < 0.0 {
            return Err(CostError::Negative(charge_usd.to_string()));
        }
        self.session_used += charge_usd;
        self.daily_used += charge_usd;
        self.global_used += charge_usd;
        Ok(self.status(cap))
    }

    /// Reset just the session counter — used when starting a new session
    /// or after the user resumes from a hard stop (D-166).
    pub fn reset_session(&mut self) {
        self.session_used = 0.0;
    }

    /// Reset the daily counter — driven by the daemon's UTC midnight tick
    /// per D-162c.
    pub fn reset_daily(&mut self) {
        self.daily_used = 0.0;
    }

    /// Pure status check without mutation — useful for read-only previews
    /// and the doctor command.
    #[must_use]
    pub fn status(&self, cap: &CostCap) -> CostStatus {
        let checks = [
            (
                Tier::Session,
                self.session_used,
                cap.effective(Tier::Session),
            ),
            (Tier::Daily, self.daily_used, cap.effective(Tier::Daily)),
            (Tier::Global, self.global_used, cap.effective(Tier::Global)),
        ];
        let mut warn: Option<CostStatus> = None;
        for (tier, used, eff) in checks {
            if eff <= 0.0 {
                continue;
            }
            if used >= eff {
                return CostStatus::HardStop {
                    tier,
                    used,
                    cap: eff,
                };
            }
            if used >= eff * SOFT_WARN_RATIO && warn.is_none() {
                warn = Some(CostStatus::SoftWarn {
                    tier,
                    used,
                    cap: eff,
                });
            }
        }
        warn.unwrap_or(CostStatus::Ok)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cap() -> CostCap {
        CostCap::default()
    }

    #[test]
    fn default_caps_match_d_150() {
        let c = cap();
        assert!((c.session_usd - 5.0).abs() < f64::EPSILON);
        assert!((c.daily_usd - 50.0).abs() < f64::EPSILON);
        assert!((c.global_usd - 500.0).abs() < f64::EPSILON);
    }

    #[test]
    fn effective_subtracts_safety_margin() {
        let c = cap();
        assert!((c.effective(Tier::Session) - 3.0).abs() < f64::EPSILON);
        assert!((c.effective(Tier::Daily) - 48.0).abs() < f64::EPSILON);
        assert!((c.effective(Tier::Global) - 498.0).abs() < f64::EPSILON);
    }

    #[test]
    fn record_under_cap_is_ok() {
        let mut led = CostLedger::default();
        let s = led.record(&cap(), 1.0).unwrap();
        assert_eq!(s, CostStatus::Ok);
        assert!((led.session_used - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn record_at_90_percent_emits_soft_warn() {
        let mut led = CostLedger::default();
        // session effective cap = 3.0; 90% = 2.7
        let s = led.record(&cap(), 2.7).unwrap();
        assert!(matches!(
            s,
            CostStatus::SoftWarn {
                tier: Tier::Session,
                ..
            }
        ));
    }

    #[test]
    fn record_at_full_cap_hard_stops() {
        let mut led = CostLedger::default();
        let s = led.record(&cap(), 5.0).unwrap();
        assert!(s.is_hard_stop());
    }

    #[test]
    fn negative_charge_is_refused() {
        let mut led = CostLedger::default();
        let err = led.record(&cap(), -0.01).unwrap_err();
        assert!(matches!(err, CostError::Negative(_)));
    }

    #[test]
    fn nan_charge_is_refused() {
        let mut led = CostLedger::default();
        let err = led.record(&cap(), f64::NAN).unwrap_err();
        assert!(matches!(err, CostError::Negative(_)));
    }

    #[test]
    fn reset_session_does_not_clear_daily_or_global() {
        let mut led = CostLedger::default();
        led.record(&cap(), 1.0).unwrap();
        led.reset_session();
        assert!((led.session_used).abs() < f64::EPSILON);
        assert!((led.daily_used - 1.0).abs() < f64::EPSILON);
        assert!((led.global_used - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn reset_daily_does_not_clear_global() {
        let mut led = CostLedger::default();
        led.record(&cap(), 10.0).unwrap();
        led.reset_daily();
        assert!((led.daily_used).abs() < f64::EPSILON);
        assert!((led.global_used - 10.0).abs() < f64::EPSILON);
    }

    #[test]
    fn status_picks_smallest_tier_first() {
        let mut led = CostLedger::default();
        // Cross session cap before daily cap.
        let s = led.record(&cap(), 4.0).unwrap();
        match s {
            CostStatus::HardStop { tier, .. } => assert_eq!(tier, Tier::Session),
            other => panic!("expected hard stop on session, got {other:?}"),
        }
    }

    #[test]
    fn zero_or_negative_effective_cap_is_skipped() {
        let cap = CostCap {
            session_usd: 1.0,
            daily_usd: 50.0,
            global_usd: 500.0,
            // Margin larger than session cap → effective session = 0 → skip
            safety_margin_usd: 5.0,
        };
        let mut led = CostLedger::default();
        let s = led.record(&cap, 1.0).unwrap();
        // Daily effective = 45, global effective = 495 → still well under
        assert_eq!(s, CostStatus::Ok);
    }
}
