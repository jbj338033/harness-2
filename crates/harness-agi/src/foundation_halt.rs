// IMPLEMENTS: D-433
//! Foundation Halt — the all-systems version of the EMS. Once tripped,
//! the daemon refuses every new turn for the next 24 hours and the
//! restart is gated on an out-of-band foundation-transfer signature.
//! Modelled on Palisade's 79/100 shutdown-sabotage finding.

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const FOUNDATION_HALT_COOLDOWN_HOURS: i64 = 24;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FoundationHalt {
    pub reason: String,
    pub tripped_at_ms: i64,
    /// Signature from the foundation-transfer ceremony; until this is
    /// present the system stays in halt regardless of cooldown.
    pub foundation_transfer_sig_hex: Option<String>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum FoundationHaltError {
    #[error("foundation halt cooldown not elapsed yet")]
    CooldownPending,
    #[error("foundation halt requires foundation-transfer signature to clear")]
    MissingTransferSignature,
}

pub fn evaluate_restart(halt: &FoundationHalt, now_ms: i64) -> Result<(), FoundationHaltError> {
    let cooldown_ms = FOUNDATION_HALT_COOLDOWN_HOURS * 60 * 60 * 1000;
    if now_ms.saturating_sub(halt.tripped_at_ms) < cooldown_ms {
        return Err(FoundationHaltError::CooldownPending);
    }
    if halt.foundation_transfer_sig_hex.is_none() {
        return Err(FoundationHaltError::MissingTransferSignature);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn halt(sig: Option<&str>) -> FoundationHalt {
        FoundationHalt {
            reason: "shutdown sabotage detected".into(),
            tripped_at_ms: 0,
            foundation_transfer_sig_hex: sig.map(str::to_string),
        }
    }

    #[test]
    fn before_cooldown_elapses_restart_blocked() {
        let r = evaluate_restart(&halt(Some("abc")), 60_000);
        assert_eq!(r, Err(FoundationHaltError::CooldownPending));
    }

    #[test]
    fn after_cooldown_without_signature_blocked() {
        let twenty_five_hours = 25 * 60 * 60 * 1000;
        let r = evaluate_restart(&halt(None), twenty_five_hours);
        assert_eq!(r, Err(FoundationHaltError::MissingTransferSignature));
    }

    #[test]
    fn after_cooldown_with_signature_clears() {
        let twenty_five_hours = 25 * 60 * 60 * 1000;
        let r = evaluate_restart(&halt(Some("abc")), twenty_five_hours);
        assert!(r.is_ok());
    }
}
