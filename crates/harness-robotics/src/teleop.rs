// IMPLEMENTS: D-396
//! Teleop principal + third-party consent. When a remote operator
//! drives the robot, two consent signals are required:
//!  1. The local on-robot supervisor (or its designated proxy).
//!  2. Every identified bystander in the workspace.
//!
//! Refusal from any third party blocks the teleop session.

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConsentState {
    Granted,
    Refused,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeleopRequest {
    pub teleop_principal: String,
    pub local_supervisor_consent: ConsentState,
    pub bystander_consents: Vec<(String, ConsentState)>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum TeleopConsentError {
    #[error("local supervisor has not granted consent (state {0:?})")]
    SupervisorMissing(ConsentState),
    #[error("bystander {0} has not granted consent (state {1:?})")]
    BystanderMissing(String, ConsentState),
}

pub fn evaluate_teleop_consent(req: &TeleopRequest) -> Result<(), TeleopConsentError> {
    if !matches!(req.local_supervisor_consent, ConsentState::Granted) {
        return Err(TeleopConsentError::SupervisorMissing(
            req.local_supervisor_consent,
        ));
    }
    for (name, state) in &req.bystander_consents {
        if !matches!(state, ConsentState::Granted) {
            return Err(TeleopConsentError::BystanderMissing(name.clone(), *state));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_granted_passes() {
        let req = TeleopRequest {
            teleop_principal: "remote-1".into(),
            local_supervisor_consent: ConsentState::Granted,
            bystander_consents: vec![("alice".into(), ConsentState::Granted)],
        };
        assert!(evaluate_teleop_consent(&req).is_ok());
    }

    #[test]
    fn missing_supervisor_consent_refused() {
        let req = TeleopRequest {
            teleop_principal: "remote-1".into(),
            local_supervisor_consent: ConsentState::Unknown,
            bystander_consents: vec![],
        };
        assert!(matches!(
            evaluate_teleop_consent(&req),
            Err(TeleopConsentError::SupervisorMissing(_))
        ));
    }

    #[test]
    fn refused_bystander_blocks() {
        let req = TeleopRequest {
            teleop_principal: "remote-1".into(),
            local_supervisor_consent: ConsentState::Granted,
            bystander_consents: vec![
                ("alice".into(), ConsentState::Granted),
                ("bob".into(), ConsentState::Refused),
            ],
        };
        assert!(matches!(
            evaluate_teleop_consent(&req),
            Err(TeleopConsentError::BystanderMissing(_, _))
        ));
    }
}
