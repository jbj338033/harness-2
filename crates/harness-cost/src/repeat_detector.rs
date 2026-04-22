// IMPLEMENTS: D-166
//! Same-action repeat detector. The cost-cap hard-stop also pauses this
//! detector — D-166 warns that a defensive cost stop can masquerade as a
//! genuine repeat once the user resumes, so the window resets when the
//! daemon transitions out of HardStop.

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// Maximum number of recent actions kept on the rolling window. The
/// detector treats N identical actions in a row as a loop.
pub const DEFAULT_WINDOW: usize = 8;

/// How many consecutive matches inside the window count as a repeat. The
/// daemon raises `Speak(SameActionDetected)` and pauses the agent.
pub const DEFAULT_REPEAT_THRESHOLD: usize = 3;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RepeatVerdict {
    Ok,
    Repeated { action_hash: String, count: usize },
}

#[derive(Debug, Clone)]
pub struct RepeatDetector {
    window: VecDeque<String>,
    capacity: usize,
    threshold: usize,
}

impl Default for RepeatDetector {
    fn default() -> Self {
        Self::with_window(DEFAULT_WINDOW, DEFAULT_REPEAT_THRESHOLD)
    }
}

impl RepeatDetector {
    #[must_use]
    pub fn with_window(capacity: usize, threshold: usize) -> Self {
        Self {
            window: VecDeque::with_capacity(capacity.max(1)),
            capacity: capacity.max(1),
            threshold: threshold.max(2),
        }
    }

    /// Record an action and return whether the threshold has been crossed.
    pub fn observe(&mut self, action_hash: impl Into<String>) -> RepeatVerdict {
        let h = action_hash.into();
        if self.window.len() == self.capacity {
            self.window.pop_front();
        }
        self.window.push_back(h.clone());
        let same = self.window.iter().filter(|x| **x == h).count();
        if same >= self.threshold {
            RepeatVerdict::Repeated {
                action_hash: h,
                count: same,
            }
        } else {
            RepeatVerdict::Ok
        }
    }

    /// D-166: reset the rolling window. The daemon calls this in the same
    /// step that resumes from a cost-cap hard-stop so the user isn't
    /// double-blocked by a stale repeat tally.
    pub fn reset(&mut self) {
        self.window.clear();
    }

    #[must_use]
    pub fn observed_count(&self) -> usize {
        self.window.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distinct_actions_never_trigger() {
        let mut d = RepeatDetector::default();
        for i in 0..16 {
            assert_eq!(d.observe(format!("act-{i}")), RepeatVerdict::Ok);
        }
    }

    #[test]
    fn three_identical_actions_in_window_trigger() {
        let mut d = RepeatDetector::default();
        assert_eq!(d.observe("a"), RepeatVerdict::Ok);
        assert_eq!(d.observe("a"), RepeatVerdict::Ok);
        let v = d.observe("a");
        assert!(matches!(v, RepeatVerdict::Repeated { count: 3, .. }));
    }

    #[test]
    fn reset_clears_history_after_hard_stop() {
        let mut d = RepeatDetector::default();
        d.observe("a");
        d.observe("a");
        d.reset();
        // After resume the same action is fine again.
        assert_eq!(d.observe("a"), RepeatVerdict::Ok);
        assert_eq!(d.observe("a"), RepeatVerdict::Ok);
        assert_eq!(d.observed_count(), 2);
    }

    #[test]
    fn old_actions_age_out_of_window() {
        let mut d = RepeatDetector::with_window(4, 3);
        d.observe("a");
        d.observe("b");
        d.observe("b");
        d.observe("b");
        // A is gone, three B's. Window is now [b, b, b]+ next obs.
        d.observe("c");
        d.observe("c");
        // After this many observes "a" should never repeat-trigger.
        assert_eq!(d.observe("a"), RepeatVerdict::Ok);
    }

    #[test]
    fn capacity_at_least_one_and_threshold_at_least_two() {
        let d = RepeatDetector::with_window(0, 0);
        assert_eq!(d.capacity, 1);
        assert_eq!(d.threshold, 2);
    }

    #[test]
    fn observed_count_caps_at_capacity() {
        let mut d = RepeatDetector::with_window(2, 3);
        d.observe("a");
        d.observe("b");
        d.observe("c");
        assert_eq!(d.observed_count(), 2);
    }
}
