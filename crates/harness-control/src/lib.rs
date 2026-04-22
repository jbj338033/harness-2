// IMPLEMENTS: D-309, D-310, D-311, D-312, D-313, D-314
//! Operator-control surface — interrupt, EMS, drift check, post-hoc
//! explanation, supervisor scheming-detector hook, consent ledger
//! VC export.
//!
//! - [`override_window`] — D-309: one-key `Ctrl-\` interrupt.
//! - [`ems`] — D-310: double-tap EMS (SIGKILL + restart) bypassing
//!   D-083 progressive shutdown.
//! - [`drift`] — D-311: long-running worker self-check at 20 turns
//!   or 30 minutes ("still on the original goal?").
//! - [`post_hoc`] — D-312: Agent Trace–compatible explanation
//!   projection.
//! - [`scheming`] — D-313: read-only Apollo-style scheming signal
//!   detector (experimental).
//! - [`vc_export`] — D-314: consent ledger → VC v2 envelope.

pub mod drift;
pub mod ems;
pub mod override_window;
pub mod post_hoc;
pub mod scheming;
pub mod vc_export;

pub use drift::{DriftCheck, DriftCheckOutcome, due_drift_check};
pub use ems::{EMS_KEY_DOUBLE_TAP_WINDOW_MS, EmsRequest, EmsTier, register_keypress};
pub use override_window::{InterruptKey, OverrideOutcome, request_interrupt};
pub use post_hoc::{ExplanationProjection, PostHocSource, build_explanation};
pub use scheming::{SchemingFinding, SchemingSignal, scan_for_scheming};
pub use vc_export::{VcCredentialSubject, VcExport, export_consent_to_vc};
