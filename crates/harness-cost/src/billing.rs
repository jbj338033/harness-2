// IMPLEMENTS: D-195, D-239
//! Cost split-brain reconciler. Two numbers run in parallel:
//!
//! - `estimate`  — Harness's per-token guess. Updated every turn.
//! - `confirmed` — what the provider's billing API later acknowledged.
//!
//! D-239 makes `confirmed` the legal authority for hard stops. The estimate
//! only ever drives early-warning UI, never the hard cap decision.
//! D-195 fixes the poll cadence so the daemon doesn't hammer the billing
//! endpoint when the estimate is well under the cap.

use crate::{CostCap, CostStatus, SOFT_WARN_RATIO, Tier};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// D-239 says estimate-only stops happen at 80% of the cap so the user is
/// warned earlier than the confirmed-billing soft warn (90%).
pub const ESTIMATE_EARLY_WARN_RATIO: f64 = 0.80;

/// D-195 default poll cadence — each provider's billing endpoint is hit
/// every five minutes when the estimate is climbing, but no more often
/// than once a turn even when the estimate spikes.
pub const DEFAULT_POLL_INTERVAL: Duration = Duration::from_secs(5 * 60);

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct ReconcileLedger {
    pub estimate_usd: f64,
    pub confirmed_usd: f64,
    pub last_polled_at_ms: i64,
}

impl ReconcileLedger {
    /// Add a per-turn Harness estimate. Caller is the writer-actor (D-174).
    pub fn record_estimate(&mut self, charge_usd: f64) {
        if charge_usd.is_finite() && charge_usd >= 0.0 {
            self.estimate_usd += charge_usd;
        }
    }

    /// Replace the confirmed total with what the provider returned.
    /// Provider numbers are absolute, not deltas — the dashboard hands us
    /// "spent so far this period" rather than "+$0.07".
    pub fn record_confirmed(&mut self, total_usd: f64, when_ms: i64) {
        if total_usd.is_finite() && total_usd >= 0.0 {
            self.confirmed_usd = total_usd;
        }
        self.last_polled_at_ms = when_ms;
    }

    /// D-239: estimate drift = absolute gap between our running guess and
    /// the provider's confirmed total. If `estimate >> confirmed` we know
    /// the provider hasn't ingested the latest turn yet (lag, normal). If
    /// `confirmed >> estimate` we under-estimated — bump the cap check.
    #[must_use]
    pub fn drift_usd(&self) -> f64 {
        self.confirmed_usd - self.estimate_usd
    }

    /// D-239: hard-stop decision uses the confirmed total only. The
    /// estimate triggers a soft warn at 80% (`ESTIMATE_EARLY_WARN_RATIO`)
    /// so the operator sees trouble before billing catches up.
    #[must_use]
    pub fn classify(&self, cap: &CostCap, tier: Tier) -> ReconcileVerdict {
        let eff = cap.effective(tier);
        if eff <= 0.0 {
            return ReconcileVerdict::Ok;
        }
        if self.confirmed_usd >= eff {
            return ReconcileVerdict::HardStop {
                source: AuthoritySource::ProviderBilling,
                used: self.confirmed_usd,
                cap: eff,
            };
        }
        // Estimate is advisory only — never escalates past soft warn.
        if self.estimate_usd >= eff * SOFT_WARN_RATIO {
            return ReconcileVerdict::SoftWarn {
                source: AuthoritySource::HarnessEstimate,
                used: self.estimate_usd,
                cap: eff,
            };
        }
        if self.estimate_usd >= eff * ESTIMATE_EARLY_WARN_RATIO {
            return ReconcileVerdict::EarlyWarn {
                source: AuthoritySource::HarnessEstimate,
                used: self.estimate_usd,
                cap: eff,
            };
        }
        ReconcileVerdict::Ok
    }

    /// Decide whether to hit the billing API right now. Always wait
    /// `interval` between polls; an empty ledger never triggers a poll
    /// (no charges yet → nothing to confirm).
    #[must_use]
    pub fn should_poll(&self, now_ms: i64, interval: Duration) -> bool {
        if self.estimate_usd <= 0.0 {
            return false;
        }
        let interval_ms = i64::try_from(interval.as_millis()).unwrap_or(i64::MAX);
        now_ms.saturating_sub(self.last_polled_at_ms) >= interval_ms
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthoritySource {
    HarnessEstimate,
    ProviderBilling,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ReconcileVerdict {
    Ok,
    EarlyWarn {
        source: AuthoritySource,
        used: f64,
        cap: f64,
    },
    SoftWarn {
        source: AuthoritySource,
        used: f64,
        cap: f64,
    },
    HardStop {
        source: AuthoritySource,
        used: f64,
        cap: f64,
    },
}

impl ReconcileVerdict {
    #[must_use]
    pub fn is_hard_stop(&self) -> bool {
        matches!(self, Self::HardStop { .. })
    }

    /// Lower a reconcile verdict to a [`CostStatus`] for callers that
    /// don't care about the authority source.
    #[must_use]
    pub fn into_status(self) -> CostStatus {
        match self {
            Self::Ok | Self::EarlyWarn { .. } => CostStatus::Ok,
            Self::SoftWarn {
                source: _,
                used,
                cap,
            } => CostStatus::SoftWarn {
                tier: Tier::Session,
                used,
                cap,
            },
            Self::HardStop {
                source: _,
                used,
                cap,
            } => CostStatus::HardStop {
                tier: Tier::Session,
                used,
                cap,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CostCap;

    fn cap() -> CostCap {
        CostCap::default()
    }

    #[test]
    fn estimate_below_threshold_is_ok() {
        let mut led = ReconcileLedger::default();
        led.record_estimate(0.5);
        assert_eq!(led.classify(&cap(), Tier::Session), ReconcileVerdict::Ok);
    }

    #[test]
    fn estimate_at_80_percent_emits_early_warn() {
        let mut led = ReconcileLedger::default();
        // session effective cap = 3.0; 80% = 2.4 — use 2.5 to avoid f64 fp noise
        led.record_estimate(2.5);
        let v = led.classify(&cap(), Tier::Session);
        assert!(
            matches!(
                v,
                ReconcileVerdict::EarlyWarn {
                    source: AuthoritySource::HarnessEstimate,
                    ..
                }
            ),
            "got {v:?}"
        );
    }

    #[test]
    fn estimate_at_90_percent_emits_soft_warn_via_estimate_only() {
        let mut led = ReconcileLedger::default();
        led.record_estimate(2.7);
        let v = led.classify(&cap(), Tier::Session);
        assert!(
            matches!(
                v,
                ReconcileVerdict::SoftWarn {
                    source: AuthoritySource::HarnessEstimate,
                    ..
                }
            ),
            "got {v:?}"
        );
    }

    #[test]
    fn estimate_alone_never_triggers_hard_stop() {
        let mut led = ReconcileLedger::default();
        led.record_estimate(1_000_000.0);
        let v = led.classify(&cap(), Tier::Session);
        assert!(!v.is_hard_stop(), "estimate must not authorise hard stop");
    }

    #[test]
    fn confirmed_at_cap_emits_hard_stop_with_provider_authority() {
        let mut led = ReconcileLedger::default();
        led.record_confirmed(3.0, 1_000);
        let v = led.classify(&cap(), Tier::Session);
        assert!(
            matches!(
                v,
                ReconcileVerdict::HardStop {
                    source: AuthoritySource::ProviderBilling,
                    ..
                }
            ),
            "got {v:?}"
        );
    }

    #[test]
    fn drift_reflects_provider_minus_estimate() {
        let mut led = ReconcileLedger::default();
        led.record_estimate(1.0);
        led.record_confirmed(1.5, 0);
        assert!((led.drift_usd() - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn negative_or_nan_inputs_are_ignored() {
        let mut led = ReconcileLedger::default();
        led.record_estimate(-1.0);
        led.record_estimate(f64::NAN);
        led.record_confirmed(-2.0, 0);
        led.record_confirmed(f64::INFINITY, 0);
        assert!((led.estimate_usd).abs() < f64::EPSILON);
        assert!(led.confirmed_usd.is_finite());
    }

    #[test]
    fn should_poll_waits_for_interval_then_fires() {
        let mut led = ReconcileLedger::default();
        led.record_estimate(0.5);
        assert!(led.should_poll(60_000, Duration::from_secs(30)));
        led.record_confirmed(0.5, 60_000);
        assert!(!led.should_poll(60_000, Duration::from_secs(30)));
        assert!(!led.should_poll(80_000, Duration::from_secs(30)));
        assert!(led.should_poll(120_000, Duration::from_secs(30)));
    }

    #[test]
    fn empty_ledger_never_polls() {
        let led = ReconcileLedger::default();
        assert!(!led.should_poll(i64::MAX, DEFAULT_POLL_INTERVAL));
    }

    #[test]
    fn into_status_collapses_authority() {
        let v = ReconcileVerdict::HardStop {
            source: AuthoritySource::ProviderBilling,
            used: 5.0,
            cap: 3.0,
        };
        assert!(v.into_status().is_hard_stop());
    }
}
