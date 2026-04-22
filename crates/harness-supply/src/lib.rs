// IMPLEMENTS: D-331, D-337, D-340
//! Three loosely coupled supply-chain hooks:
//!
//! - [`scan`] — sweep a skill / MCP package directory for prompt
//!   injection markers. ToxicSkills (2026-02) found 36% of public
//!   skills carried at least one injection-class phrase, so D-331 makes
//!   this gate mandatory before activation.
//! - [`reversal`] — generate the operator playbook a destructive
//!   action triggers under EU PLD 2026-12 strict liability.
//! - [`disclosure`] — emit per-jurisdiction legal text when a skill's
//!   `risk_tier` and `applicable_jurisdictions` overlap the user's
//!   region (D-340).

pub mod disclosure;
pub mod reversal;
pub mod scan;

pub use disclosure::{Jurisdiction, LegalDisclosure, RiskTier, SkillMeta, disclosures_for};
pub use reversal::{ReversalStep, ReversalWorkflow, build_reversal};
pub use scan::{ScanFinding, ScanReport, scan_skill_dir};
