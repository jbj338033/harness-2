// IMPLEMENTS: D-369, D-370, D-371, D-372, D-373, D-374, D-375
//! Finance mode (axis off by default).
//!
//! - [`scope`] — D-369: hard scope at "analyst assistant". Order entry,
//!   personalised investment advice, and discretionary recommendations
//!   are refused.
//! - [`tier`] — D-370: separate `FinancialTier` enum (kept distinct
//!   from R40's `PaymentTier` so the two can never be confused at
//!   call sites).
//! - [`integrity`] — D-371: every numeric claim must carry a citation
//!   AND must round-trip when checked against its source. Failure
//!   blocks emission rather than silently passing.
//! - [`pii`] — D-372: account / card number redaction default ON.
//! - [`risk`] — D-373: per-user VaR / volatility constraints stored as
//!   a typed memory row.
//! - [`audit`] — D-374: finance event with export adapters for SR 11-7
//!   (Fed model risk), FEAT (HKMA), FRIA (EU), FINRA, FCA.
//! - [`disclosure`] — D-375: AI washing guard (SEC 2024 enforcement
//!   pattern) — disallows "AI-driven", "powered by AI" puffery
//!   without a substantive claim.

pub mod audit;
pub mod disclosure;
pub mod integrity;
pub mod pii;
pub mod risk;
pub mod scope;
pub mod tier;

pub use audit::{AuditExportFormat, FinanceAuditEvent, export_audit};
pub use disclosure::{AiWashingVerdict, screen_disclosure};
pub use integrity::{NumericClaim, NumericIntegrityError, verify_numeric_claim};
pub use pii::{FinancePiiHit, FinancePiiKind, redact_finance_pii};
pub use risk::{RiskConstraintRow, RiskMetric};
pub use scope::{FinanceScopeError, FinanceTask, ensure_in_scope};
pub use tier::FinancialTier;
