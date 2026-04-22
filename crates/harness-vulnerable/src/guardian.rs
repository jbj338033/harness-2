// IMPLEMENTS: D-402
//! Guardian escalation with three orthogonal consent axes:
//!  * `Parent` — natural / legal guardian
//!  * `Aps` — Adult Protective Services contact
//!  * `SchoolOfficial` — school-employed designated contact
//!
//! At least one axis must hold a *Granted* consent for guardian-tier
//! actions to proceed. The escalation flow itself sits between D-382
//! (crisis) and D-314 (consent ledger).

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConsentAxis {
    Parent,
    Aps,
    SchoolOfficial,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConsentState {
    Unknown,
    Granted,
    Revoked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuardianConsent {
    pub axis: ConsentAxis,
    pub state: ConsentState,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum GuardianConsentError {
    #[error("no guardian axis has granted consent — escalation refused")]
    NoneGranted,
}

pub fn evaluate_guardian_consent(
    consents: &[GuardianConsent],
) -> Result<ConsentAxis, GuardianConsentError> {
    consents
        .iter()
        .find(|c| matches!(c.state, ConsentState::Granted))
        .map(|c| c.axis)
        .ok_or(GuardianConsentError::NoneGranted)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(axis: ConsentAxis, state: ConsentState) -> GuardianConsent {
        GuardianConsent { axis, state }
    }

    #[test]
    fn one_granted_returns_that_axis() {
        let r = evaluate_guardian_consent(&[
            c(ConsentAxis::Parent, ConsentState::Unknown),
            c(ConsentAxis::SchoolOfficial, ConsentState::Granted),
        ])
        .unwrap();
        assert_eq!(r, ConsentAxis::SchoolOfficial);
    }

    #[test]
    fn all_unknown_or_revoked_refused() {
        let r = evaluate_guardian_consent(&[
            c(ConsentAxis::Parent, ConsentState::Unknown),
            c(ConsentAxis::Aps, ConsentState::Revoked),
        ]);
        assert!(matches!(r, Err(GuardianConsentError::NoneGranted)));
    }
}
