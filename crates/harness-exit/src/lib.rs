// IMPLEMENTS: D-418, D-419, D-420, D-421, D-422
//! Exit / lifecycle surface.
//!
//! - [`export`] — D-418: `harness export` envelope + Agent Trace
//!   sidecar pin so the pinned trace travels with the export
//!   bundle (GDPR Art 20 + D-318 re-use).
//! - [`lifecycle`] — D-419: `ProjectLifecycleState` + 90/180-day
//!   GitHub-Actions dead-man switch.
//! - [`release`] — D-420: SLSA L3 + 2-of-2 detached signature gate.
//! - [`governance`] — D-421: pinned governance file list (TRADEMARK,
//!   FORK, GOVERNANCE, ARCHIVE_POLICY).
//! - [`final_migration`] — D-422: final migration release contract
//!   activated when the project transitions to `MaintenanceOnly`.

pub mod export;
pub mod final_migration;
pub mod governance;
pub mod lifecycle;
pub mod release;

pub use export::{ExportBundle, build_export_bundle};
pub use final_migration::{
    FINAL_MIGRATION_DOC_PATH, FinalMigrationContract, requires_final_migration,
};
pub use governance::{GOVERNANCE_FILES, governance_file_count};
pub use lifecycle::{
    DEAD_MAN_INACTIVITY_TRIGGER_DAYS, DEAD_MAN_WARN_DAYS, ProjectLifecycleState, classify_dead_man,
};
pub use release::{REQUIRED_SIGNERS, ReleaseSignature, SlsaLevel, evaluate_release};
