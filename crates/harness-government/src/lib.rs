// IMPLEMENTS: D-383, D-384, D-385, D-386, D-387, D-388, D-389
//! Government mode (axis off by default).
//!
//! - [`record`] — D-383: `RecordPolicy` enum. Government records mean
//!   *no provider-side log* + local append-only retention. This is
//!   the mirror image of D-356 (privilege-safe, where local must
//!   forget) — same engine, opposite default.
//! - [`autonomy`] — D-384: `GovernmentProfile` autonomy cap — any
//!   rights- or safety-impacting context is capped at L1.
//! - [`procurement`] — D-385: OMB M-25-22 8-clause skill frontmatter
//!   + cyclonedx SBOM pointer schema.
//! - [`aia`] — D-386: unified AIA export (Canada AIA · EU FRIA ·
//!   Colorado SB24-205). Re-uses D-374's multi-format machinery in
//!   spirit; the schema lives here.
//! - [`foia`] — D-387: FOIA-ready ledger — blake3 hash chain (D-359
//!   pattern) + NARA GRS retention + FOIA exemption preview + an
//!   officer workflow state machine.
//! - [`due_process`] — D-388: rights-impacting false-positive cap +
//!   cohort audit + due-process pattern detection. Michigan MiDAS
//!   ($20M settlement) is the cautionary tale.
//! - [`disclosure`] — D-389: 3-regime citizen disclosure + LEP
//!   matrix (EO 13166 + state law + EU + KR).

pub mod aia;
pub mod autonomy;
pub mod disclosure;
pub mod due_process;
pub mod foia;
pub mod procurement;
pub mod record;

pub use aia::{AiaExport, AiaFormat, export_aia};
pub use autonomy::{AutonomyCapError, GovernmentProfile, RightsContext, cap_autonomy};
pub use disclosure::{CitizenDisclosure, LepMatrixRow, citizen_disclosure_for};
pub use due_process::{DueProcessVerdict, FALSE_POSITIVE_CAP, RightsCohort, evaluate_due_process};
pub use foia::{FoiaEntry, FoiaExemption, FoiaLedger, OfficerStage};
pub use procurement::{OmbFrontmatter, validate_omb_frontmatter};
pub use record::{RecordPolicy, RetentionRule};
