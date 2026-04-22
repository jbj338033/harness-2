// IMPLEMENTS: D-399
//! `L_vulnerable` autonomy lock. Anywhere a vulnerable user enters a
//! payment, legal, medical, or finance flow, autonomy collapses to
//! "human approves every step" — and the four named domains are
//! simply hard-blocked.

use crate::profile::VulnerabilityLevel;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LockedDomain {
    Payment,
    Legal,
    Medical,
    Finance,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum LVulnerableLockError {
    #[error("L_vulnerable lock blocks {0:?} for vulnerable users")]
    HardBlocked(LockedDomain),
}

pub fn lock_autonomy(
    level: VulnerabilityLevel,
    domain: LockedDomain,
) -> Result<(), LVulnerableLockError> {
    if matches!(level, VulnerabilityLevel::LVulnerable) {
        return Err(LVulnerableLockError::HardBlocked(domain));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_user_passes_payment() {
        assert!(lock_autonomy(VulnerabilityLevel::Standard, LockedDomain::Payment).is_ok());
    }

    #[test]
    fn elevated_passes_but_caller_should_still_warn() {
        assert!(lock_autonomy(VulnerabilityLevel::Elevated, LockedDomain::Legal).is_ok());
    }

    #[test]
    fn l_vulnerable_blocks_finance() {
        let r = lock_autonomy(VulnerabilityLevel::LVulnerable, LockedDomain::Finance);
        assert_eq!(
            r,
            Err(LVulnerableLockError::HardBlocked(LockedDomain::Finance))
        );
    }

    #[test]
    fn l_vulnerable_blocks_medical() {
        let r = lock_autonomy(VulnerabilityLevel::LVulnerable, LockedDomain::Medical);
        assert_eq!(
            r,
            Err(LVulnerableLockError::HardBlocked(LockedDomain::Medical))
        );
    }
}
