// IMPLEMENTS: D-251, D-252, D-253, D-254, D-255, D-256, D-257, D-258, D-259, D-260
//! DevOps / SRE primitives.
//!
//! - [`alert`] — D-251: normalised `IncidentAlert` schema
//!   (severity / labels / service / runbook ref).
//! - [`attachment`] — D-252: incident attachment variants
//!   (StackTrace, LogBundle, MetricSample, TraceSpan) — these mirror
//!   the D-156 multimodal scaffold but live here so SRE tools don't
//!   re-import the multimodal crate just for log handles.
//! - [`incident`] — D-253: first-class `Incident` entity.
//! - [`tier`] — D-254: `ToolTier`. The new axis sits beside D-012's
//!   sandbox triple — it gates kubectl / terraform / cloud-CLI tools.
//! - [`tools`] — D-255: registry pointer struct for the
//!   `harness-tools-kubectl`, `-terraform`, `-cloudcli` crates.
//! - [`presets`] — D-256: 5 incident-skill role presets.
//! - [`error_budget`] — D-257: SLO burn-rate trigger.
//! - [`oncall`] — D-258: on-call recall request schema.
//! - [`a2a`] — D-259: SRE A2A endpoint registration descriptor.
//! - [`runbook_map`] — D-260: runbook-alert → skill mapping table.

pub mod a2a;
pub mod alert;
pub mod attachment;
pub mod error_budget;
pub mod incident;
pub mod oncall;
pub mod presets;
pub mod runbook_map;
pub mod tier;
pub mod tools;

pub use a2a::{A2aEndpoint, A2aProvider};
pub use alert::{AlertSeverity, IncidentAlert};
pub use attachment::IncidentAttachment;
pub use error_budget::{BurnRateVerdict, ErrorBudgetPolicy, evaluate_burn_rate};
pub use incident::{Incident, IncidentStatus, TimelineEntry};
pub use oncall::{OnCallProvider, OnCallRecallRequest};
pub use presets::{IncidentRolePreset, all_incident_presets};
pub use runbook_map::{RunbookSkillMap, lookup_skill};
pub use tier::ToolTier;
pub use tools::SreToolCrate;
