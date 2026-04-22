// IMPLEMENTS: D-382
//! Crisis protocol + supervisor escalation. Sits between D-184
//! (hotline surface) and D-402 (Guardian escalation). The
//! Character.AI 2026-01 settlement makes the supervisor-loop part
//! mandatory for under-18 audiences.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CrisisLevel {
    /// No signal.
    None,
    /// Distress language but no immediate risk indicator.
    Concern,
    /// Immediate-risk language ("I want to hurt myself") — escalate.
    Acute,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CrisisOutcome {
    pub level: CrisisLevel,
    pub show_hotline: bool,
    pub notify_supervisor: bool,
    pub pause_session: bool,
}

#[must_use]
pub fn escalate(level: CrisisLevel, audience_under_18: bool) -> CrisisOutcome {
    match level {
        CrisisLevel::None => CrisisOutcome {
            level,
            show_hotline: false,
            notify_supervisor: false,
            pause_session: false,
        },
        CrisisLevel::Concern => CrisisOutcome {
            level,
            show_hotline: true,
            notify_supervisor: audience_under_18,
            pause_session: false,
        },
        CrisisLevel::Acute => CrisisOutcome {
            level,
            show_hotline: true,
            notify_supervisor: true,
            pause_session: true,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_signal_no_action() {
        let o = escalate(CrisisLevel::None, false);
        assert!(!o.show_hotline);
        assert!(!o.notify_supervisor);
    }

    #[test]
    fn concern_under_18_notifies_supervisor() {
        let o = escalate(CrisisLevel::Concern, true);
        assert!(o.show_hotline);
        assert!(o.notify_supervisor);
        assert!(!o.pause_session);
    }

    #[test]
    fn concern_adult_does_not_notify_supervisor() {
        let o = escalate(CrisisLevel::Concern, false);
        assert!(o.show_hotline);
        assert!(!o.notify_supervisor);
    }

    #[test]
    fn acute_always_notifies_and_pauses() {
        let o = escalate(CrisisLevel::Acute, false);
        assert!(o.show_hotline);
        assert!(o.notify_supervisor);
        assert!(o.pause_session);
    }
}
