// IMPLEMENTS: D-394
//! 3-tier Emergency Stop. Each tier is independent of the others —
//! the SW EMS (D-310) is fallback, the robot-level software stop is
//! middle, the **HW E-stop** is final and certified to ISO 13850
//! Cat 0/1, SIL 3 / PL e. Even if every software path fails, pulling
//! the HW button must take effect.
//!
//! `request_estop` records what was asked and which tier responded;
//! the actual hardware is wired by the platform integrator.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EstopTier {
    /// Software EMS — host-side cancellation (D-310). Fallback only.
    SoftwareEms,
    /// Robot-level software stop — controller refuses new motion.
    RobotSoftware,
    /// Hardware E-stop — ISO 13850 Cat 0/1, SIL 3 / PL e. Final.
    HardwareIso13850,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EmergencyStop {
    pub reason: String,
    pub at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EstopOutcome {
    pub tier: EstopTier,
    pub stop: EmergencyStop,
    /// `true` if a higher-authority tier MUST also confirm. Always
    /// true unless the request originated from the HW tier.
    pub upgrade_required: bool,
}

#[must_use]
pub fn request_estop(stop: EmergencyStop, tier: EstopTier) -> EstopOutcome {
    EstopOutcome {
        upgrade_required: !matches!(tier, EstopTier::HardwareIso13850),
        tier,
        stop,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stop() -> EmergencyStop {
        EmergencyStop {
            reason: "operator pressed".into(),
            at_ms: 1,
        }
    }

    #[test]
    fn hw_tier_does_not_require_upgrade() {
        let o = request_estop(stop(), EstopTier::HardwareIso13850);
        assert!(!o.upgrade_required);
    }

    #[test]
    fn sw_tier_requires_upgrade_to_higher() {
        let o = request_estop(stop(), EstopTier::SoftwareEms);
        assert!(o.upgrade_required);
    }

    #[test]
    fn tier_ordering_is_sw_then_robot_then_hw() {
        assert!(EstopTier::SoftwareEms < EstopTier::RobotSoftware);
        assert!(EstopTier::RobotSoftware < EstopTier::HardwareIso13850);
    }
}
