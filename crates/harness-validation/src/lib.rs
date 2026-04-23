// IMPLEMENTS: D-153, D-192, D-193, D-213
//! Validation primitives.
//!
//! - [`decision_dict`] — D-153: live `DecisionDictionary` with the
//!   wave correlation id, mutable revisions surface to every Worker
//!   in the same wave through `memory.pages[wave:<id>:decisions]`.
//! - [`compiled_truth`] — D-192: external verifier for the
//!   compiled-truth artifacts (monthly cadence). Hash-cross-check
//!   against the published manifest.
//! - [`macos_fuzz`] — D-193: macOS sandbox behavioural fuzz. The
//!   matrix lists OS versions × surface and a verdict for each cell.
//! - [`ci_simulation`] — D-213: 1000-user CI workload simulation
//!   plan + verdict.

pub mod ci_simulation;
pub mod compiled_truth;
pub mod decision_dict;
pub mod macos_fuzz;

pub use ci_simulation::{CiSimulationPlan, CiSimulationResult, evaluate_simulation};
pub use compiled_truth::{
    CompiledTruthManifest, CompiledTruthVerifyError, CompiledTruthVerifyOutcome, verify_truth,
};
pub use decision_dict::{
    DecisionDictionary, DecisionDictionaryError, DecisionEntry, MEMORY_PAGE_KEY_PREFIX,
    apply_revision, page_key,
};
pub use macos_fuzz::{MacOsFuzzCell, MacOsFuzzMatrix, MacOsFuzzVerdict, evaluate_fuzz};
