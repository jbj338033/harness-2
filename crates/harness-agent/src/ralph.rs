use serde::{Deserialize, Serialize};

pub const NO_CHANGE_LIMIT: u32 = 3;

pub const SAME_ERROR_LIMIT: u32 = 5;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RalphSignal {
    FilesChanged,
    NoChange,
    Error { class: String },
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct CircuitBreaker {
    no_change_streak: u32,
    last_error: Option<String>,
    same_error_streak: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BreakerVerdict {
    Continue,
    Stalled,
    Stuck { class: String },
}

impl CircuitBreaker {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn evaluate(&mut self, signal: RalphSignal) -> BreakerVerdict {
        match signal {
            RalphSignal::FilesChanged => {
                self.no_change_streak = 0;
                self.last_error = None;
                self.same_error_streak = 0;
                BreakerVerdict::Continue
            }
            RalphSignal::NoChange => {
                self.no_change_streak = self.no_change_streak.saturating_add(1);
                self.last_error = None;
                self.same_error_streak = 0;
                if self.no_change_streak >= NO_CHANGE_LIMIT {
                    BreakerVerdict::Stalled
                } else {
                    BreakerVerdict::Continue
                }
            }
            RalphSignal::Error { class } => {
                self.no_change_streak = 0;
                if self.last_error.as_deref() == Some(class.as_str()) {
                    self.same_error_streak = self.same_error_streak.saturating_add(1);
                } else {
                    self.same_error_streak = 1;
                    self.last_error = Some(class.clone());
                }
                if self.same_error_streak >= SAME_ERROR_LIMIT {
                    BreakerVerdict::Stuck { class }
                } else {
                    BreakerVerdict::Continue
                }
            }
        }
    }

    #[must_use]
    pub fn no_change_streak(&self) -> u32 {
        self.no_change_streak
    }

    #[must_use]
    pub fn same_error_streak(&self) -> u32 {
        self.same_error_streak
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn err(class: &str) -> RalphSignal {
        RalphSignal::Error {
            class: class.into(),
        }
    }

    #[test]
    fn file_changes_reset_everything() {
        let mut cb = CircuitBreaker::new();
        assert_eq!(cb.evaluate(RalphSignal::NoChange), BreakerVerdict::Continue);
        assert_eq!(cb.evaluate(RalphSignal::NoChange), BreakerVerdict::Continue);
        assert_eq!(
            cb.evaluate(RalphSignal::FilesChanged),
            BreakerVerdict::Continue
        );
        assert_eq!(cb.no_change_streak(), 0);
    }

    #[test]
    fn three_no_changes_trip_stalled() {
        let mut cb = CircuitBreaker::new();
        assert_eq!(cb.evaluate(RalphSignal::NoChange), BreakerVerdict::Continue);
        assert_eq!(cb.evaluate(RalphSignal::NoChange), BreakerVerdict::Continue);
        assert_eq!(cb.evaluate(RalphSignal::NoChange), BreakerVerdict::Stalled);
    }

    #[test]
    fn five_same_errors_trip_stuck() {
        let mut cb = CircuitBreaker::new();
        for _ in 0..4 {
            assert_eq!(cb.evaluate(err("E0308")), BreakerVerdict::Continue);
        }
        assert_eq!(
            cb.evaluate(err("E0308")),
            BreakerVerdict::Stuck {
                class: "E0308".into()
            }
        );
    }

    #[test]
    fn different_errors_dont_trip() {
        let mut cb = CircuitBreaker::new();
        for i in 0..10 {
            let class = format!("E{i}");
            assert_eq!(cb.evaluate(err(&class)), BreakerVerdict::Continue);
        }
    }

    #[test]
    fn alternating_errors_reset() {
        let mut cb = CircuitBreaker::new();
        cb.evaluate(err("A"));
        cb.evaluate(err("A"));
        cb.evaluate(err("B"));
        for _ in 0..4 {
            assert_eq!(cb.evaluate(err("A")), BreakerVerdict::Continue);
        }
        assert_eq!(
            cb.evaluate(err("A")),
            BreakerVerdict::Stuck { class: "A".into() }
        );
    }

    #[test]
    fn file_change_after_near_miss() {
        let mut cb = CircuitBreaker::new();
        cb.evaluate(RalphSignal::NoChange);
        cb.evaluate(RalphSignal::NoChange);
        cb.evaluate(RalphSignal::FilesChanged);
        assert_eq!(cb.evaluate(RalphSignal::NoChange), BreakerVerdict::Continue);
    }
}
