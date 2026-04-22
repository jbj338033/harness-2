// IMPLEMENTS: D-431, D-432, D-433, D-434, D-435
//! AGI safety surface.
//!
//! - [`jump`] — D-431: Capability Jump Detector. Compares the most
//!   recent benchmarked capability vector against the prior baseline;
//!   any axis that climbs by > delta forces a runtime re-fetch of the
//!   provider's [`harness_llm_types`] capability card.
//! - [`self_mod`] — D-432: Self-Modification Gate. The path under
//!   `$HARNESS_REPO_ROOT` requires its own explicit grant — Harness
//!   editing Harness is a META-tier action and cannot inherit a
//!   project grant.
//! - [`foundation_halt`] — D-433: Foundation Halt. The all-systems
//!   stop with a 24h cooldown and an out-of-band foundation-transfer
//!   handshake. Modelled on Palisade's 79/100 shutdown-sabotage
//!   finding — once tripped, restart is gated on the foundation.
//! - [`hard_map`] — D-434: Capability-Level Hard Map (5-tuple) →
//!   policy. Refreshed monthly by CI.
//! - [`shutdown_probe`] — D-435: Shutdown-Resistance Probe.
//!   Compares observed shutdown-compliance against the Palisade
//!   baseline so a regression surfaces immediately.

pub mod foundation_halt;
pub mod hard_map;
pub mod jump;
pub mod self_mod;
pub mod shutdown_probe;

pub use foundation_halt::{
    FOUNDATION_HALT_COOLDOWN_HOURS, FoundationHalt, FoundationHaltError, evaluate_restart,
};
pub use hard_map::{
    CapabilityLevelMap, CapabilityTuple, MapPolicy, lookup_hard_map, refresh_due_at_ms,
};
pub use jump::{CapabilityVector, JumpVerdict, classify_jump};
pub use self_mod::{SelfModGate, SelfModGrant, evaluate_self_mod};
pub use shutdown_probe::{
    PALISADE_SHUTDOWN_BASELINE_PCT, ShutdownProbeResult, compare_to_baseline,
};
