// IMPLEMENTS: D-424, D-425, D-426, D-427, D-428, D-429, D-430
//! Foundation surface — branding, provider resilience, liability,
//! insurance.
//!
//! - [`branding`] — D-424: "분신/bunshin" terminology is forbidden;
//!   the canonical term is "agent runtime".
//! - [`fallback`] — D-425: 5-tier provider fallback ladder. Used to
//!   keep the daemon working when any one provider (eg. Anthropic
//!   ~40% concentration) goes down.
//! - [`local_first`] — D-426: local inference fallback as a
//!   first-class citizen.
//! - [`gpai_parser`] — D-427: GPAI Code of Practice safety-info
//!   parser.
//! - [`collapse`] — D-428: provider-collapse 30-minute trigger that
//!   suggests an Agent Trace export.
//! - [`liability`] — D-429: 5-language Liability & Insurance README
//!   pinned set (gross-negligence civil-law fallback for AGPL
//!   "as-is").
//! - [`insurance_export`] — D-430: 90-day Agent Trace retention +
//!   insurance / legal export envelope.

pub mod branding;
pub mod collapse;
pub mod fallback;
pub mod gpai_parser;
pub mod insurance_export;
pub mod liability;
pub mod local_first;

pub use branding::{BANNED_TERMS, BrandingFinding, CANONICAL_TERM, scan_for_banned_terms};
pub use collapse::{
    PROVIDER_COLLAPSE_TRIGGER_MINUTES, ProviderCollapseRecommendation, evaluate_collapse,
};
pub use fallback::{ProviderFallbackTier, all_fallback_tiers, next_tier};
pub use gpai_parser::{GpaiSafetyEntry, parse_gpai_safety_block};
pub use insurance_export::{AGENT_TRACE_RETENTION_DAYS, InsuranceExport, build_insurance_export};
pub use liability::{LIABILITY_LANGUAGES, liability_doc_path};
pub use local_first::{LocalInferenceProfile, default_local_profile};
