// IMPLEMENTS: D-378
//! COPPA under-13 gate. The 2026-04-22 enforcement deadline made this
//! mandatory. Default is REFUSE — only with verifiable parental
//! consent on file may an under-13 session proceed, and even then
//! only with the educator-supervised crisis protocol enabled.

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoppaDecision {
    /// Adult or 13+ — passes the gate.
    Allow,
    /// Under-13 with verifiable parental consent + supervised crisis
    /// protocol — passes.
    AllowSupervised,
    /// Under-13 without consent — refused.
    RefuseUnder13NoConsent,
    /// Under-13 with consent but the supervisor / crisis protocol is
    /// off — refused.
    RefuseUnder13UnsafeProtocol,
}

#[derive(Debug, Error)]
pub enum CoppaError {
    #[error("under-13 session refused: {0:?}")]
    Refused(CoppaDecision),
}

#[must_use]
pub fn evaluate_coppa(
    is_under_13: bool,
    has_parental_consent: bool,
    crisis_protocol_on: bool,
) -> CoppaDecision {
    if !is_under_13 {
        return CoppaDecision::Allow;
    }
    if !has_parental_consent {
        return CoppaDecision::RefuseUnder13NoConsent;
    }
    if !crisis_protocol_on {
        return CoppaDecision::RefuseUnder13UnsafeProtocol;
    }
    CoppaDecision::AllowSupervised
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adult_passes() {
        assert_eq!(evaluate_coppa(false, false, false), CoppaDecision::Allow);
    }

    #[test]
    fn under_13_no_consent_refused() {
        assert_eq!(
            evaluate_coppa(true, false, true),
            CoppaDecision::RefuseUnder13NoConsent
        );
    }

    #[test]
    fn under_13_consent_no_crisis_refused() {
        assert_eq!(
            evaluate_coppa(true, true, false),
            CoppaDecision::RefuseUnder13UnsafeProtocol
        );
    }

    #[test]
    fn under_13_consent_with_crisis_allowed_supervised() {
        assert_eq!(
            evaluate_coppa(true, true, true),
            CoppaDecision::AllowSupervised
        );
    }
}
