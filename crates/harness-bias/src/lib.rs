// IMPLEMENTS: D-300, D-301, D-302, D-303, D-304, D-305, D-306, D-307
//! Bias audit + FMEA release gate.
//!
//! - [`cli`] — D-300: shape of the `harness audit bias` invocation.
//! - [`cohort`] — D-301: opt-in cohort audit + NYC Local Law 144 /
//!   FRIA export.
//! - [`provider_profile`] — D-302: provider-by-provider BBQ /
//!   StereoSet score table.
//! - [`skill_lint`] — D-303: skill linter (WinoBias + axe-core
//!   stub) enumerating violations.
//! - [`role_bbq`] — D-304: per-preset BBQ threshold gate.
//! - [`memory_scan`] — D-305: periodic 8-axis memory scan schedule.
//! - [`disclosure`] — D-306: README 3-line disclosure (employment /
//!   credit / public-safety + biometric + deep-chatbot exclusion).
//! - [`fmea`] — D-307: FMEA RPN gate with a bias axis.

pub mod cli;
pub mod cohort;
pub mod disclosure;
pub mod fmea;
pub mod memory_scan;
pub mod provider_profile;
pub mod role_bbq;
pub mod skill_lint;

pub use cli::{BiasAuditFormat, BiasAuditInvocation, parse_invocation};
pub use cohort::{CohortAuditExport, CohortAuditFormat, export_cohort_audit};
pub use disclosure::{README_DISCLOSURE_LINE_COUNT, README_DISCLOSURE_LINES};
pub use fmea::{FmeaEntry, FmeaVerdict, RPN_GATE, evaluate_fmea};
pub use memory_scan::{MemoryScanSchedule, NEXT_SCAN_INTERVAL_HOURS, due_at_ms};
pub use provider_profile::{ProviderBiasProfile, all_provider_profiles};
pub use role_bbq::{ROLE_BBQ_THRESHOLD, RoleBbqVerdict, evaluate_role_bbq};
pub use skill_lint::{SkillLintFinding, SkillLintRule, lint_skill};
