// IMPLEMENTS: D-410, D-411, D-412, D-413, D-414, D-415, D-416, D-417
//! Business / community surface.
//!
//! - [`dashboard`] — D-410: quarterly transparency dashboard schema.
//! - [`revenue`] — D-411: ASC 606 / IFRS 15 monthly contract-
//!   liability amortisation.
//! - [`templates`] — D-412: commercial contract template registry
//!   (MSA · DPA · Enterprise SOW).
//! - [`tax`] — D-413: multi-jurisdiction tax filing checklist (IRS
//!   5472, 한국 조특법 17조, EU VAT OSS, JP QII).
//! - [`incorporation`] — D-414: triple-gate incorporation trigger
//!   (first paid contract / MAU 1k / Sponsor $3k/mo).
//! - [`cla`] — D-415: DCO + Harmony CLA 1.0 dual-track gate.
//! - [`partnership`] — D-416: partnership milestone ladder.
//! - [`coc`] — D-417: Contributor Covenant v2.1 enforcement
//!   workflow with the second-mediator escalation step.

pub mod cla;
pub mod coc;
pub mod dashboard;
pub mod incorporation;
pub mod partnership;
pub mod revenue;
pub mod tax;
pub mod templates;

pub use cla::{ClaError, ContributionTrack, evaluate_contribution};
pub use coc::{CocAction, CocStage, advance_coc};
pub use dashboard::{QUARTERLY_DASHBOARD_SECTIONS, QuarterlyDashboard, dashboard_section_count};
pub use incorporation::{
    IncorporationGate, IncorporationVerdict, MAU_GATE, MONTHLY_SPONSOR_USD_GATE,
    evaluate_incorporation,
};
pub use partnership::{PartnershipMilestone, all_partnership_milestones};
pub use revenue::{ContractLiabilityRow, monthly_recognition};
pub use tax::{TaxFiling, all_tax_filings};
pub use templates::{ContractTemplate, registered_templates};
