// IMPLEMENTS: D-117, D-200
//! Stewardship primitives.
//!
//! - [`maintenance_lock`] — D-117: maintenance-lock mutex spec for
//!   `~/.harness/state/maintenance.lock`. The lock is held while a
//!   non-resumable operation (eg. SQLite VACUUM) runs; if the
//!   operation is interrupted the recovery path is rollback + from
//!   scratch.
//! - [`maintainers`] — D-200: co-maintainer admission gate. Two
//!   active maintainers must hold valid Harmony CLA signatures
//!   before the project can advance to the next-step process
//!   defined in D-220 / D-422.

pub mod maintainers;
pub mod maintenance_lock;

pub use maintainers::{
    CLA_VERSION, CoMaintainerSlate, MaintainerError, MaintainerRecord, evaluate_co_maintainers,
};
pub use maintenance_lock::{
    LockOutcome, MAINTENANCE_LOCK_PATH, MaintenanceLockState, RecoveryStrategy, recover,
};
