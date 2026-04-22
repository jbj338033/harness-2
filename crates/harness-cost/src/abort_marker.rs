// IMPLEMENTS: D-174
//! Atomic ledger update + abort-marker pair. D-174 resolved the cross-patch
//! between cost-cap multi-session sums (D-162) and the scheduled-abort
//! marker (D-167) — both must move together inside the writer-actor's
//! single update path so a reader never sees "ledger over cap, but no
//! abort marker yet" or vice versa.

use crate::{CostCap, CostLedger, CostStatus, Tier};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AbortReason {
    SessionCap,
    DailyCap,
    GlobalCap,
}

impl From<Tier> for AbortReason {
    fn from(t: Tier) -> Self {
        match t {
            Tier::Session => Self::SessionCap,
            Tier::Daily => Self::DailyCap,
            Tier::Global => Self::GlobalCap,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AbortMarker {
    pub reason: AbortReason,
    pub used_usd: f64,
    pub cap_usd: f64,
    pub set_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UpdateOutcome {
    pub status: CostStatus,
    pub abort_marker: Option<AbortMarker>,
}

/// Apply a charge to the ledger and produce the (status, abort marker)
/// pair the writer-actor commits inside one transaction. The marker is
/// `Some` exactly when the post-update status is HardStop, so a reader
/// can decide "do I need to roll the turn back?" from a single field.
pub fn apply_charge_with_marker(
    ledger: &mut CostLedger,
    cap: &CostCap,
    charge_usd: f64,
    now_ms: i64,
) -> UpdateOutcome {
    let status = match ledger.record(cap, charge_usd) {
        Ok(s) => s,
        Err(_) => CostStatus::Ok,
    };
    let abort_marker = match &status {
        CostStatus::HardStop { tier, used, cap } => Some(AbortMarker {
            reason: AbortReason::from(*tier),
            used_usd: *used,
            cap_usd: *cap,
            set_at_ms: now_ms,
        }),
        _ => None,
    };
    UpdateOutcome {
        status,
        abort_marker,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cap() -> CostCap {
        CostCap::default()
    }

    #[test]
    fn ok_charge_yields_no_marker() {
        let mut led = CostLedger::default();
        let out = apply_charge_with_marker(&mut led, &cap(), 0.5, 100);
        assert_eq!(out.status, CostStatus::Ok);
        assert!(out.abort_marker.is_none());
    }

    #[test]
    fn hard_stop_charge_yields_marker_in_same_call() {
        let mut led = CostLedger::default();
        let out = apply_charge_with_marker(&mut led, &cap(), 5.0, 1_700);
        assert!(out.status.is_hard_stop());
        let marker = out.abort_marker.expect("hard stop must produce marker");
        assert_eq!(marker.reason, AbortReason::SessionCap);
        assert_eq!(marker.set_at_ms, 1_700);
        assert!((marker.used_usd - 5.0).abs() < f64::EPSILON);
    }

    #[test]
    fn soft_warn_does_not_produce_marker() {
        let mut led = CostLedger::default();
        let out = apply_charge_with_marker(&mut led, &cap(), 2.7, 1);
        assert!(matches!(out.status, CostStatus::SoftWarn { .. }));
        assert!(out.abort_marker.is_none());
    }

    #[test]
    fn negative_charge_does_not_perturb_ledger_or_marker() {
        let mut led = CostLedger::default();
        let out = apply_charge_with_marker(&mut led, &cap(), -1.0, 0);
        assert_eq!(out.status, CostStatus::Ok);
        assert!(out.abort_marker.is_none());
        assert!((led.session_used).abs() < f64::EPSILON);
    }

    #[test]
    fn abort_reason_round_trips_via_serde() {
        let m = AbortMarker {
            reason: AbortReason::DailyCap,
            used_usd: 100.0,
            cap_usd: 48.0,
            set_at_ms: 42,
        };
        let s = serde_json::to_string(&m).unwrap();
        assert!(s.contains("\"reason\":\"daily_cap\""));
        let back: AbortMarker = serde_json::from_str(&s).unwrap();
        assert_eq!(back, m);
    }
}
