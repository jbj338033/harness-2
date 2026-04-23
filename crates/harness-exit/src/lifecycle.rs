// IMPLEMENTS: D-419
//! `ProjectLifecycleState` + 90/180-day dead-man switch. A GitHub
//! Actions cron walks the repo daily; at 90 days of inactivity we
//! move to `Warn`, and at 180 days to `MaintenanceOnly`. The
//! maintainer can manually reset to `Active`.

use serde::{Deserialize, Serialize};

pub const DEAD_MAN_WARN_DAYS: u32 = 90;
pub const DEAD_MAN_INACTIVITY_TRIGGER_DAYS: u32 = 180;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectLifecycleState {
    Active,
    Warn,
    MaintenanceOnly,
    Archived,
}

#[must_use]
pub fn classify_dead_man(days_since_last_commit: u32) -> ProjectLifecycleState {
    if days_since_last_commit >= DEAD_MAN_INACTIVITY_TRIGGER_DAYS {
        ProjectLifecycleState::MaintenanceOnly
    } else if days_since_last_commit >= DEAD_MAN_WARN_DAYS {
        ProjectLifecycleState::Warn
    } else {
        ProjectLifecycleState::Active
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_repo_is_active() {
        assert_eq!(classify_dead_man(10), ProjectLifecycleState::Active);
    }

    #[test]
    fn ninety_days_warns() {
        assert_eq!(
            classify_dead_man(DEAD_MAN_WARN_DAYS),
            ProjectLifecycleState::Warn
        );
    }

    #[test]
    fn hundred_eighty_days_maintenance_only() {
        assert_eq!(
            classify_dead_man(DEAD_MAN_INACTIVITY_TRIGGER_DAYS),
            ProjectLifecycleState::MaintenanceOnly
        );
    }
}
