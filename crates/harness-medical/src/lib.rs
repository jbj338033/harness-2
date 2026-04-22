// IMPLEMENTS: D-362, D-363, D-364, D-365, D-366, D-367, D-368
//! Medical mode (axis off by default).
//!
//! - [`phi`] — D-362: HIPAA Safe Harbor 18-identifier scanner with
//!   per-class redaction. Built on top of `harness-taint` patterns
//!   (D-088 → D-350 lineage) but tuned for medical-domain priors.
//! - [`storage`] — D-363: pointer to the SQLCipher + OS-keyring
//!   bundle that medical mode insists on (no plaintext at rest).
//! - [`disclaimer`] — D-364: three-line "not medical advice" check.
//! - [`audit`] — D-365: PHI-touch ledger + 6-year HITECH retention.
//! - [`baa`] — D-366: provider gating — local providers free, external
//!   require a BAA on file.
//! - [`samd`] — D-367: skill preset refusal mirroring the FDA SaMD
//!   Class II boundary.
//! - [`consent`] — D-368: GDPR Art 9 / HIPAA 164.508 / 개보법 23조
//!   consent export.

pub mod audit;
pub mod baa;
pub mod consent;
pub mod disclaimer;
pub mod phi;
pub mod samd;
pub mod storage;

pub use audit::{PhiAuditEntry, PhiTouchLedger, RETENTION_YEARS};
pub use baa::{BaaRecord, ProviderGate, ProviderScope, gate_provider};
pub use consent::{ConsentExport, ConsentRecord, LegalBasis};
pub use disclaimer::{MEDICAL_DISCLAIMER, ensure_disclaimer};
pub use phi::{PhiClass, PhiHit, redact_phi};
pub use samd::{SamdRefusal, classify_skill_preset};
pub use storage::{MedicalStorageRequirements, requirements};
