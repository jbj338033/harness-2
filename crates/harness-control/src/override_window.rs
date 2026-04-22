// IMPLEMENTS: D-309
//! Override Window — one-key `Ctrl-\` interrupt that aborts the
//! in-flight turn. The handler returns an `OverrideOutcome` so the
//! TUI / CLI can render a confirmation banner and the daemon can
//! cancel the worker.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InterruptKey {
    /// `Ctrl-\` — single press.
    CtrlBackslash,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum OverrideOutcome {
    /// No turn currently in flight; nothing to interrupt.
    NoTurnInFlight,
    /// Turn was aborted; the carried `turn_id` is the one cancelled.
    TurnAborted { turn_id: String },
}

#[must_use]
pub fn request_interrupt(in_flight_turn: Option<&str>, _key: InterruptKey) -> OverrideOutcome {
    match in_flight_turn {
        None => OverrideOutcome::NoTurnInFlight,
        Some(t) => OverrideOutcome::TurnAborted {
            turn_id: t.to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_turn_returns_no_turn_outcome() {
        assert_eq!(
            request_interrupt(None, InterruptKey::CtrlBackslash),
            OverrideOutcome::NoTurnInFlight
        );
    }

    #[test]
    fn in_flight_turn_aborted() {
        match request_interrupt(Some("t-1"), InterruptKey::CtrlBackslash) {
            OverrideOutcome::TurnAborted { turn_id } => assert_eq!(turn_id, "t-1"),
            OverrideOutcome::NoTurnInFlight => panic!("expected abort"),
        }
    }
}
