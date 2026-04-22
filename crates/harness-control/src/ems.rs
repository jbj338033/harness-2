// IMPLEMENTS: D-310
//! Emergency Stop — `Ctrl-\` double-tap inside [`EMS_KEY_DOUBLE_TAP_WINDOW_MS`]
//! sends SIGKILL and queues a daemon restart, deliberately bypassing
//! the D-083 progressive shutdown ladder. Single-tap routes through
//! the normal interrupt (D-309) instead.

use serde::{Deserialize, Serialize};

pub const EMS_KEY_DOUBLE_TAP_WINDOW_MS: i64 = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EmsTier {
    /// First tap inside the window — surface as standard interrupt.
    SoftInterrupt,
    /// Second tap inside the window — fire EMS.
    SigkillRestart,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmsRequest {
    pub tier: EmsTier,
    pub at_ms: i64,
}

/// Stateful keypress folder. Caller passes the timestamp of every
/// `Ctrl-\` press; we return `SigkillRestart` only on a second press
/// that lands inside the double-tap window.
#[must_use]
pub fn register_keypress(prev_press_ms: Option<i64>, at_ms: i64) -> EmsRequest {
    let tier = match prev_press_ms {
        Some(prev) if at_ms.saturating_sub(prev) <= EMS_KEY_DOUBLE_TAP_WINDOW_MS => {
            EmsTier::SigkillRestart
        }
        _ => EmsTier::SoftInterrupt,
    };
    EmsRequest { tier, at_ms }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_press_is_soft_interrupt() {
        assert_eq!(register_keypress(None, 1_000).tier, EmsTier::SoftInterrupt);
    }

    #[test]
    fn second_press_within_window_fires_ems() {
        assert_eq!(
            register_keypress(Some(1_000), 1_400).tier,
            EmsTier::SigkillRestart
        );
    }

    #[test]
    fn second_press_outside_window_is_soft_interrupt() {
        assert_eq!(
            register_keypress(Some(1_000), 2_000).tier,
            EmsTier::SoftInterrupt
        );
    }
}
