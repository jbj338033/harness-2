// IMPLEMENTS: D-185, D-186, D-188, D-189, D-423
//! Lifecycle rights surface.
//!
//! - [`art22`] — D-185: GDPR Art 22 right-to-explanation export.
//! - [`memory_bias`] — D-186: agent-written memory bias audit
//!   (8-axis distribution scan + drift score).
//! - [`legacy`] — D-188: `~/.harness/legacy.toml` posthumous policy
//!   (Cambridge 2024 griefbot study is the cautionary tale).
//! - [`retire`] — D-189: `harness retire` identity wipe via
//!   crypto-shredding (D-218 marks this as the one approved
//!   projection-invariant break).
//! - [`exit_doctor`] — D-423: `harness doctor --exit` checklist.

pub mod art22;
pub mod exit_doctor;
pub mod legacy;
pub mod memory_bias;
pub mod retire;

pub use art22::{Art22ExplanationExport, build_art22_explanation};
pub use exit_doctor::{ExitDoctorReport, ExitDoctorStep, run_exit_doctor};
pub use legacy::{LegacyPolicy, LegacyTrigger, parse_legacy_toml};
pub use memory_bias::{BiasAxisCount, MemoryBiasAuditReport, audit_memory_bias};
pub use retire::{RetireOutcome, RetireRequest, perform_retire};
