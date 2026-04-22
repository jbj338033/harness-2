// IMPLEMENTS: D-397, D-398, D-399, D-400, D-401, D-402, D-403
//! Vulnerable users mode (axis off by default).
//!
//! - [`profile`] — D-397: `VulnerabilityProfile` — age × cognitive ×
//!   guardian × trust band. Unknown defaults to *Elevated* so the
//!   safe path is the easy path.
//! - [`signals`] — D-398: 6-signal vulnerability escalator. Signals
//!   are *raise-only*, the raw waveforms are *volatile* (never
//!   persisted), the user is informed, and a one-click disable is
//!   advertised back.
//! - [`autonomy`] — D-399: `L_vulnerable` autonomy lock — payment,
//!   legal, medical, finance domains are hard-blocked.
//! - [`dark_patterns`] — D-400: AADC 7-pattern CI gate. Any release
//!   that lights up a pattern fails CI.
//! - [`cognitive`] — D-401: cognitive skill metadata.
//! - [`guardian`] — D-402: Guardian escalation with three consent
//!   axes (parent / APS / school-official).
//! - [`disclosure`] — D-403: README line + `harness whoami` row +
//!   issue template pointer.

pub mod autonomy;
pub mod cognitive;
pub mod dark_patterns;
pub mod disclosure;
pub mod guardian;
pub mod profile;
pub mod signals;

pub use autonomy::{LVulnerableLockError, LockedDomain, lock_autonomy};
pub use cognitive::{CognitiveSkillMeta, validate_cognitive_meta};
pub use dark_patterns::{DarkPatternFinding, DarkPatternKind, scan_for_aadc_violations};
pub use disclosure::{
    DISCLOSURE_README_LINE, DISCLOSURE_VULN_ISSUE_TEMPLATE_PATH, VulnWhoAmIRow, vuln_whoami_row,
};
pub use guardian::{ConsentAxis, GuardianConsent, evaluate_guardian_consent};
pub use profile::{
    AgeBand, CognitiveBand, GuardianBand, TrustBand, VulnerabilityLevel, VulnerabilityProfile,
};
pub use signals::{Signal, SignalEscalator, SignalKind};
