// IMPLEMENTS: D-395
//! Sim-to-real gate. A policy / model trained only in simulation
//! cannot move to the real robot until it carries a deployment tag
//! that asserts a successful sim-to-real evaluation. Trying to deploy
//! a `SimulationOnly` policy to `RealHardware` is refused.

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeploymentTag {
    SimulationOnly,
    /// Policy passed sim-to-real evaluation on the named platform.
    SimToRealApproved,
    /// Real-hardware deployment authorised by safety review.
    RealHardware,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SimToRealError {
    #[error("simulation-only policy may not run on real hardware")]
    SimulationOnlyToReal,
    #[error("downgrade from real-hardware to simulation needs explicit reset")]
    UnauthorisedDowngrade,
}

pub fn gate_sim_to_real(
    policy_tag: DeploymentTag,
    target: DeploymentTag,
) -> Result<(), SimToRealError> {
    match (policy_tag, target) {
        (DeploymentTag::SimulationOnly, DeploymentTag::RealHardware) => {
            Err(SimToRealError::SimulationOnlyToReal)
        }
        (DeploymentTag::RealHardware, DeploymentTag::SimulationOnly) => {
            Err(SimToRealError::UnauthorisedDowngrade)
        }
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simulation_only_to_real_blocked() {
        let r = gate_sim_to_real(DeploymentTag::SimulationOnly, DeploymentTag::RealHardware);
        assert_eq!(r, Err(SimToRealError::SimulationOnlyToReal));
    }

    #[test]
    fn approved_to_real_passes() {
        let r = gate_sim_to_real(
            DeploymentTag::SimToRealApproved,
            DeploymentTag::RealHardware,
        );
        assert!(r.is_ok());
    }

    #[test]
    fn real_to_simulation_downgrade_blocked() {
        let r = gate_sim_to_real(DeploymentTag::RealHardware, DeploymentTag::SimulationOnly);
        assert_eq!(r, Err(SimToRealError::UnauthorisedDowngrade));
    }
}
