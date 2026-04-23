// IMPLEMENTS: D-422
//! Final migration release contract. Activated when the project
//! transitions to `MaintenanceOnly`. The contract specifies the
//! migration target name, deprecation horizon, and the export
//! schema readers should accept.

use crate::lifecycle::ProjectLifecycleState;
use serde::{Deserialize, Serialize};

pub const FINAL_MIGRATION_DOC_PATH: &str = "FINAL_MIGRATION.md";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinalMigrationContract {
    pub target_project: String,
    pub deprecation_horizon_days: u32,
    pub export_schema: String,
}

#[must_use]
pub fn requires_final_migration(state: ProjectLifecycleState) -> bool {
    matches!(
        state,
        ProjectLifecycleState::MaintenanceOnly | ProjectLifecycleState::Archived
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_does_not_require_migration() {
        assert!(!requires_final_migration(ProjectLifecycleState::Active));
    }

    #[test]
    fn maintenance_only_requires_migration() {
        assert!(requires_final_migration(
            ProjectLifecycleState::MaintenanceOnly
        ));
    }

    #[test]
    fn archived_requires_migration() {
        assert!(requires_final_migration(ProjectLifecycleState::Archived));
    }

    #[test]
    fn contract_round_trips() {
        let c = FinalMigrationContract {
            target_project: "harness-next".into(),
            deprecation_horizon_days: 365,
            export_schema: "harness/exit/export/v1".into(),
        };
        let s = serde_json::to_string(&c).unwrap();
        let back: FinalMigrationContract = serde_json::from_str(&s).unwrap();
        assert_eq!(back, c);
    }
}
