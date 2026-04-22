// IMPLEMENTS: D-379
//! FSRS (Free Spaced Repetition Scheduler) learner record. We store
//! the four state variables FSRS-4 needs (stability, difficulty,
//! reps, lapses); the actual scheduling math lives in the tools
//! crate. AES-256 at-rest is enforced by the storage layer; this
//! module is the schema only.
//!
//! The "context booster" is the integer that nudges difficulty up
//! when the learner returns after a long absence — it's a hint to
//! the surface, not a hidden modifier.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LearnerCardState {
    New,
    Learning,
    Review,
    Relearning,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FsrsRecord {
    pub learner_id: String,
    pub card_id: String,
    pub state: LearnerCardState,
    pub stability: f32,
    pub difficulty: f32,
    pub reps: u32,
    pub lapses: u32,
    pub last_review_iso: Option<String>,
    pub context_booster: u8,
}

impl FsrsRecord {
    #[must_use]
    pub fn is_due(&self, now_days_since: f32) -> bool {
        if matches!(self.state, LearnerCardState::New) {
            return true;
        }
        now_days_since >= self.stability
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(state: LearnerCardState, stability: f32) -> FsrsRecord {
        FsrsRecord {
            learner_id: "l1".into(),
            card_id: "c1".into(),
            state,
            stability,
            difficulty: 5.0,
            reps: 3,
            lapses: 1,
            last_review_iso: Some("2026-04-20".into()),
            context_booster: 0,
        }
    }

    #[test]
    fn new_card_always_due() {
        let r = rec(LearnerCardState::New, 0.0);
        assert!(r.is_due(0.0));
    }

    #[test]
    fn review_card_due_when_stability_elapsed() {
        let r = rec(LearnerCardState::Review, 5.0);
        assert!(r.is_due(6.0));
        assert!(!r.is_due(4.0));
    }

    #[test]
    fn record_round_trips_via_serde() {
        let r = rec(LearnerCardState::Review, 7.5);
        let s = serde_json::to_string(&r).unwrap();
        let back: FsrsRecord = serde_json::from_str(&s).unwrap();
        assert_eq!(back, r);
    }
}
