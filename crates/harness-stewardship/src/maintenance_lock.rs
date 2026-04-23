// IMPLEMENTS: D-117
//! Maintenance lock mutex spec. The lockfile lives at
//! [`MAINTENANCE_LOCK_PATH`] (relative to `~/.harness/state/`). The
//! daemon refuses any session work while the lockfile exists; if the
//! lock owner died mid-operation, the recovery strategy depends on
//! whether the operation was resumable.

use serde::{Deserialize, Serialize};

pub const MAINTENANCE_LOCK_PATH: &str = "state/maintenance.lock";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MaintenanceLockState {
    Free,
    HeldResumable,
    HeldNonResumable,
    OrphanedResumable,
    OrphanedNonResumable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LockOutcome {
    /// Caller may proceed.
    AcquireOk,
    /// Lock is genuinely busy with an active owner.
    Busy,
    /// Lock is orphaned and recoverable; daemon should follow
    /// [`recover`].
    NeedsRecovery,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryStrategy {
    Resume,
    /// Non-resumable operations (eg. SQLite VACUUM) must roll back
    /// and rerun from scratch.
    RollbackFromScratch,
}

#[must_use]
pub fn classify_acquire(state: MaintenanceLockState) -> LockOutcome {
    match state {
        MaintenanceLockState::Free => LockOutcome::AcquireOk,
        MaintenanceLockState::HeldResumable | MaintenanceLockState::HeldNonResumable => {
            LockOutcome::Busy
        }
        MaintenanceLockState::OrphanedResumable | MaintenanceLockState::OrphanedNonResumable => {
            LockOutcome::NeedsRecovery
        }
    }
}

#[must_use]
pub fn recover(state: MaintenanceLockState) -> Option<RecoveryStrategy> {
    match state {
        MaintenanceLockState::OrphanedResumable => Some(RecoveryStrategy::Resume),
        MaintenanceLockState::OrphanedNonResumable => Some(RecoveryStrategy::RollbackFromScratch),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lock_path_is_under_state() {
        assert_eq!(MAINTENANCE_LOCK_PATH, "state/maintenance.lock");
    }

    #[test]
    fn free_state_acquires() {
        assert_eq!(
            classify_acquire(MaintenanceLockState::Free),
            LockOutcome::AcquireOk
        );
    }

    #[test]
    fn live_owner_returns_busy() {
        assert_eq!(
            classify_acquire(MaintenanceLockState::HeldResumable),
            LockOutcome::Busy
        );
    }

    #[test]
    fn orphan_state_routes_to_recovery() {
        assert_eq!(
            classify_acquire(MaintenanceLockState::OrphanedNonResumable),
            LockOutcome::NeedsRecovery
        );
    }

    #[test]
    fn resumable_orphan_resumes() {
        assert_eq!(
            recover(MaintenanceLockState::OrphanedResumable),
            Some(RecoveryStrategy::Resume)
        );
    }

    #[test]
    fn non_resumable_orphan_rolls_back() {
        assert_eq!(
            recover(MaintenanceLockState::OrphanedNonResumable),
            Some(RecoveryStrategy::RollbackFromScratch)
        );
    }

    #[test]
    fn live_owner_has_no_recovery() {
        assert!(recover(MaintenanceLockState::HeldResumable).is_none());
    }
}
