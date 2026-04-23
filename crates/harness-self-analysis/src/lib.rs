// IMPLEMENTS: D-353, D-354
//! Self-analysis surface.
//!
//! - [`codeql`] — D-353: CodeQL query metadata + QLCoder
//!   experimental flag. The actual scans run in CI; this module is
//!   the metadata the gate consumes.
//! - [`ra_ap`] — D-354: in-process Rust self-analysis through the
//!   `ra_ap_*` crates. We list the analyses we run and a
//!   `SelfAnalysisFinding` envelope.

pub mod codeql;
pub mod ra_ap;

pub use codeql::{
    CodeqlPack, CodeqlQuery, CodeqlSeverity, codeql_query_count, registered_codeql_pack,
};
pub use ra_ap::{RaApAnalysis, SelfAnalysisFinding, classify_findings, registered_ra_ap_analyses};
