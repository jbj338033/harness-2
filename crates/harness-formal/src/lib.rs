// IMPLEMENTS: D-324, D-325
//! Formal-methods surface.
//!
//! - [`stateright`] — D-324: enumerates the (TurnPhase × CrashTime)
//!   product so the Stateright runner can sweep every cell. The
//!   actual model check lives in `tests/stateright_turn_phases.rs`;
//!   this module is the cell descriptor every cell must visit.
//! - [`lean`] — D-325: AI-assisted Lean 4 proof descriptors. Names
//!   the invariants, the source-translation toolchain
//!   (Aeneas / Thrust Rust → Lean), and the tactic providers
//!   (DeepSeek-Prover-V2, Leanstral). Status is recorded as
//!   `Pending / Discharged / OpenSubgoal`.

pub mod lean;
pub mod stateright;

pub use lean::{LeanProofObligation, LeanProofStatus, RustToLeanToolchain, TacticProvider};
pub use stateright::{CrashTime, ModelCheckCell, TurnPhase, all_cells, cell_count};
