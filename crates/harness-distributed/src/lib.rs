// IMPLEMENTS: D-442, D-443, D-444, D-445, D-446, D-447, D-449
//! Open-weight model registry + distributed primitives.
//!
//! - [`capability`] — D-442: runtime-measured `ModelCapability`
//!   replaces the static tier table.
//! - [`safety`] — D-443: pointer to `harness-safety` (Llama Guard 4 +
//!   Prompt Guard 2) with the three turn-loop hook positions baked
//!   into the schema.
//! - [`korea`] — D-444: Korean-provider preset descriptor (Solar
//!   Pro 2, Kanana, HyperCLOVA X SEED, KT Midm) + locale/region gate.
//! - [`registry`] — D-445: `model_registry.toml` row schema with the
//!   8 required compliance fields.
//! - [`no_consensus`] — D-446: explicit refusal to ship Raft/Paxos/
//!   Zab. Single logical writer per session is the invariant.
//! - [`hlc`] — D-447: 64-bit hybrid logical clock (48-bit physical
//!   ms + 16-bit counter).
//! - [`crdt`] — D-449: the two CRDT types we ship — `LwwRegister`
//!   and `OrSet`.

pub mod capability;
pub mod crdt;
pub mod hlc;
pub mod korea;
pub mod no_consensus;
pub mod registry;
pub mod safety;

pub use capability::{CapabilityCorpus, MeasuredCapability, classify_capability};
pub use crdt::{LwwRegister, OrSet};
pub use hlc::{Hlc, HlcError, HlcTimestamp, max_hlc, tick_with};
pub use korea::{KoreaPreset, all_korea_presets, gate_korea_locale};
pub use no_consensus::{NoConsensusContract, refuse_consensus_protocol};
pub use registry::{BisTier, RegistryRow, validate_registry_row};
pub use safety::{LlamaGuardSpec, SafetyHookPoint, all_safety_hook_points};
