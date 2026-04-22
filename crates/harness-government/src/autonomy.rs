// IMPLEMENTS: D-384
//! Government autonomy cap. Anywhere the context is rights-impacting
//! (benefits, immigration, criminal justice, child welfare, voting)
//! or safety-impacting (emergency dispatch, public health), autonomy
//! is capped at L1 — the model proposes; a human must approve every
//! consequential action.

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RightsContext {
    Routine,
    /// Decision affects rights (benefits/immigration/criminal/etc.).
    RightsImpacting,
    /// Decision affects life safety.
    SafetyImpacting,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct AutonomyLevel(pub u8);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GovernmentProfile {
    pub requested: AutonomyLevel,
    pub context: RightsContext,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum AutonomyCapError {
    #[error("requested L{requested} exceeds cap L{cap} for {context:?}")]
    AboveCap {
        requested: u8,
        cap: u8,
        context: RightsContext,
    },
}

#[must_use]
pub fn cap_for(context: RightsContext) -> AutonomyLevel {
    match context {
        RightsContext::Routine => AutonomyLevel(3),
        RightsContext::RightsImpacting | RightsContext::SafetyImpacting => AutonomyLevel(1),
    }
}

pub fn cap_autonomy(profile: GovernmentProfile) -> Result<AutonomyLevel, AutonomyCapError> {
    let cap = cap_for(profile.context);
    if profile.requested > cap {
        return Err(AutonomyCapError::AboveCap {
            requested: profile.requested.0,
            cap: cap.0,
            context: profile.context,
        });
    }
    Ok(profile.requested)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rights_context_caps_at_l1() {
        let r = cap_autonomy(GovernmentProfile {
            requested: AutonomyLevel(3),
            context: RightsContext::RightsImpacting,
        });
        assert!(matches!(r, Err(AutonomyCapError::AboveCap { cap: 1, .. })));
    }

    #[test]
    fn safety_context_caps_at_l1() {
        let r = cap_autonomy(GovernmentProfile {
            requested: AutonomyLevel(2),
            context: RightsContext::SafetyImpacting,
        });
        assert!(matches!(r, Err(AutonomyCapError::AboveCap { cap: 1, .. })));
    }

    #[test]
    fn routine_allows_up_to_l3() {
        let r = cap_autonomy(GovernmentProfile {
            requested: AutonomyLevel(3),
            context: RightsContext::Routine,
        });
        assert_eq!(r.unwrap(), AutonomyLevel(3));
    }

    #[test]
    fn rights_l1_passes() {
        let r = cap_autonomy(GovernmentProfile {
            requested: AutonomyLevel(1),
            context: RightsContext::RightsImpacting,
        });
        assert!(r.is_ok());
    }
}
