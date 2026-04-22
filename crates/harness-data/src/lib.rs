// IMPLEMENTS: D-261, D-262, D-263, D-264, D-265, D-266, D-267
//! Data engineering surface (the four native tool crates' shared
//! types).
//!
//! - [`tools`] — D-261: registry pointer for `harness-tools-sql`,
//!   `-dataframe`, `-notebook`, `-pipeline`. Catalogue / semantic /
//!   quality / lineage are intentionally MCP-delegated to keep the
//!   1-person OSS scope manageable.
//! - [`sql_tier`] — D-262: 3-tier DB sandbox (`ReadOnlyProd /
//!   Staging / Sandbox`) with a static danger detector — the
//!   `sqlparser-rs` integration lives in the tool crate; this module
//!   provides the keyword-level pre-filter.
//! - [`schema`] — D-263: schema projection memory rows for
//!   text-to-SQL grounding.
//! - [`dataframe`] — D-264: dataframe runtime selector — pandas
//!   default, Polars opt-in.
//! - [`semantic`] — D-265: text-to-SQL semantic layer pointer (MCP
//!   only — we deliberately don't reinvent Cortex Analyst / dbt
//!   Copilot).
//! - [`benchmark`] — D-266: benchmark honesty guard — refuses to
//!   parrot ">90% GA" without naming the canonical sub-50% datasets.
//! - [`lineage`] — D-267: OpenLineage / DataHub MCP stretch
//!   descriptor.

pub mod benchmark;
pub mod dataframe;
pub mod lineage;
pub mod schema;
pub mod semantic;
pub mod sql_tier;
pub mod tools;

pub use benchmark::{BenchmarkClaim, BenchmarkVerdict, screen_benchmark_claim};
pub use dataframe::{DataframeRuntime, default_runtime};
pub use lineage::{LineageProvider, LineageStretchPointer};
pub use schema::{ColumnRow, ForeignKeyRow, SchemaProjection, SchemaRole, TableRow};
pub use semantic::{SemanticAdapter, SemanticAdapterError, register_semantic_adapter};
pub use sql_tier::{SqlSandboxTier, SqlStaticDanger, classify_sql};
pub use tools::{DataToolCrate, registered_data_crates};
