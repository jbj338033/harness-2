// IMPLEMENTS: D-191
//! Approval habituation guard. After ten consecutive allow decisions the
//! user is probably clicking through without reading. D-191 slows the
//! next prompt with a 1-3 second randomised delay and re-renders the
//! full action context so habituation breaks before something dangerous
//! slips past.

use serde::{Deserialize, Serialize};
use std::time::Duration;

pub const HABITUATION_THRESHOLD: u32 = 10;
pub const MIN_THROTTLE: Duration = Duration::from_secs(1);
pub const MAX_THROTTLE: Duration = Duration::from_secs(3);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalDecision {
    Allow,
    Deny,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HabituationGuard {
    pub streak: u32,
    pub triggered_count: u32,
}

impl HabituationGuard {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Log a user decision. Allow decisions advance the streak; any Deny
    /// resets it to zero so clicking "no" doesn't feed the habit meter.
    pub fn record(&mut self, decision: ApprovalDecision) {
        match decision {
            ApprovalDecision::Allow => {
                self.streak += 1;
                if self.streak >= HABITUATION_THRESHOLD
                    && self.streak.is_multiple_of(HABITUATION_THRESHOLD)
                {
                    self.triggered_count += 1;
                }
            }
            ApprovalDecision::Deny => {
                self.streak = 0;
            }
        }
    }

    /// Should the next prompt be throttled? True iff the streak has hit
    /// the threshold and hasn't been reset by a deny since.
    #[must_use]
    pub fn should_throttle(&self) -> bool {
        self.streak >= HABITUATION_THRESHOLD && self.streak.is_multiple_of(HABITUATION_THRESHOLD)
    }
}

/// Deterministic delay picked from `[MIN_THROTTLE, MAX_THROTTLE]` based
/// on the current streak. Using the streak rather than a real RNG lets
/// tests pin behaviour while still varying across firings.
#[must_use]
pub fn throttle_delay(streak: u32) -> Duration {
    let min_ms = MIN_THROTTLE.as_millis();
    let max_ms = MAX_THROTTLE.as_millis();
    let span = max_ms.saturating_sub(min_ms).max(1);
    let jitter = u128::from(streak.wrapping_mul(2_654_435_761)) % span;
    let ms = min_ms + jitter;
    Duration::from_millis(u64::try_from(ms).unwrap_or(u64::MAX))
}

/// Message to re-render alongside the throttle — caller localises it via
/// `harness-i18n` at display time.
pub const HABITUATION_NOTICE_KEY: &str = "system_message.habituation_break";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deny_resets_streak() {
        let mut g = HabituationGuard::new();
        for _ in 0..5 {
            g.record(ApprovalDecision::Allow);
        }
        assert_eq!(g.streak, 5);
        g.record(ApprovalDecision::Deny);
        assert_eq!(g.streak, 0);
    }

    #[test]
    fn streak_below_threshold_does_not_throttle() {
        let mut g = HabituationGuard::new();
        for _ in 0..(HABITUATION_THRESHOLD - 1) {
            g.record(ApprovalDecision::Allow);
        }
        assert!(!g.should_throttle());
    }

    #[test]
    fn streak_at_threshold_triggers_throttle() {
        let mut g = HabituationGuard::new();
        for _ in 0..HABITUATION_THRESHOLD {
            g.record(ApprovalDecision::Allow);
        }
        assert!(g.should_throttle());
        assert_eq!(g.triggered_count, 1);
    }

    #[test]
    fn second_multiple_of_threshold_triggers_again() {
        let mut g = HabituationGuard::new();
        for _ in 0..(HABITUATION_THRESHOLD * 2) {
            g.record(ApprovalDecision::Allow);
        }
        assert_eq!(g.triggered_count, 2);
    }

    #[test]
    fn delay_is_between_one_and_three_seconds() {
        for s in 0..1000 {
            let d = throttle_delay(s);
            assert!(d >= MIN_THROTTLE);
            assert!(d <= MAX_THROTTLE);
        }
    }

    #[test]
    fn delay_varies_across_streak_values() {
        let d1 = throttle_delay(10);
        let d2 = throttle_delay(20);
        assert_ne!(d1, d2);
    }
}
