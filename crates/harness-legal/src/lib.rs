// IMPLEMENTS: D-355, D-356, D-357, D-358, D-359, D-360, D-361
//! Legal mode (axis off by default).
//!
//! - [`citation`] — D-355: typestate `UnverifiedCitation → VerifiedCitation`
//!   so a hallucinated case never escapes. Johnson v. Dunn (2025-07
//!   disqualification) is the cautionary tale we're hardened against.
//! - [`privilege`] — D-356: client-confidential taint that travels with
//!   any value derived from privileged input, plus a provider manifest
//!   gate. US v. Heppner (2026-02) showed that "we asked the model" is
//!   not enough to preserve privilege; only providers in the manifest
//!   may receive tainted bytes.
//! - [`matter`] — D-357: `Matter` primitive — every legal session is
//!   scoped to one matter (jurisdiction + memory wall + Chinese-wall
//!   exclusions).
//! - [`upl`] — D-358: three-line UPL defence (disclaimer, output
//!   filter for "you should sue / I represent you" patterns,
//!   supervisor attestation).
//! - [`audit`] — D-359: per-matter append-only ledger with a blake3
//!   hash chain so any tampering breaks the link.
//! - [`redline`] — D-360: clause-library aware DOCX redline planner
//!   (we only emit the *plan*; DOCX serialisation lives in the
//!   tools crate).
//! - [`search`] — D-361: legal search request with PII gating that
//!   any `harness-tools-legal-search` adapter consumes.

pub mod audit;
pub mod citation;
pub mod matter;
pub mod privilege;
pub mod redline;
pub mod search;
pub mod upl;

pub use audit::{AuditEntry, MatterLedger};
pub use citation::{CitationError, CitationSource, UnverifiedCitation, VerifiedCitation};
pub use matter::{ChineseWall, Jurisdiction, Matter, MatterId, MatterScopeError};
pub use privilege::{PrivilegeError, PrivilegeManifest, PrivilegeTaint, ProviderId};
pub use redline::{ClauseLibrary, RedlineOp, RedlinePlan};
pub use search::{LegalSearchRequest, PiiVerdict, redact_request};
pub use upl::{UplVerdict, screen_output};
