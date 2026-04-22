// IMPLEMENTS: D-377
//! Socratic turn loop with a Vygotsky ZPD hint scaler.
//!
//! `hint_level` is 0–4, mapped over three ZPD bands:
//!  * 0–1: Independent — the learner should solve unaided.
//!  * 2–3: Zone of Proximal Development — scaffolded hints.
//!  * 4: Beyond ZPD — show worked-example fragment.
//!
//! `scale_hint` reads the recent attempt history and returns the next
//! hint level. Doubling the hint each attempt overshoots; the rule is
//! "step up only after a wrong attempt, step down after a streak of
//! correct attempts to keep the learner inside the ZPD".

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct HintLevel(pub u8);

impl HintLevel {
    #[must_use]
    pub fn band(self) -> ZpdBand {
        match self.0 {
            0..=1 => ZpdBand::Independent,
            2..=3 => ZpdBand::Zpd,
            _ => ZpdBand::WorkedExample,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ZpdBand {
    Independent,
    Zpd,
    WorkedExample,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct NextHint {
    pub level: HintLevel,
    pub band: ZpdBand,
}

#[must_use]
pub fn scale_hint(current: HintLevel, last_attempt_correct: bool, correct_streak: u8) -> NextHint {
    let raw = if last_attempt_correct {
        if correct_streak >= 2 {
            current.0.saturating_sub(1)
        } else {
            current.0
        }
    } else {
        (current.0 + 1).min(4)
    };
    let level = HintLevel(raw);
    NextHint {
        level,
        band: level.band(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrong_answer_steps_up_one() {
        let n = scale_hint(HintLevel(1), false, 0);
        assert_eq!(n.level, HintLevel(2));
        assert_eq!(n.band, ZpdBand::Zpd);
    }

    #[test]
    fn streak_of_two_correct_steps_down() {
        let n = scale_hint(HintLevel(3), true, 2);
        assert_eq!(n.level, HintLevel(2));
    }

    #[test]
    fn one_correct_holds_level() {
        let n = scale_hint(HintLevel(2), true, 1);
        assert_eq!(n.level, HintLevel(2));
    }

    #[test]
    fn cap_at_four() {
        let n = scale_hint(HintLevel(4), false, 0);
        assert_eq!(n.level, HintLevel(4));
        assert_eq!(n.band, ZpdBand::WorkedExample);
    }

    #[test]
    fn cannot_drop_below_zero() {
        let n = scale_hint(HintLevel(0), true, 5);
        assert_eq!(n.level, HintLevel(0));
        assert_eq!(n.band, ZpdBand::Independent);
    }
}
