// IMPLEMENTS: D-149, D-155, D-157
//! Managed + scheduled agents, the A2A protocol envelope, and the
//! Windows AppContainer sandbox descriptor.
//!
//! - [`managed`] — D-149: managed (detached daemon) and scheduled
//!   (cron + cross-run `memory.pages[slug]`) agent primitives.
//! - [`a2a`] — D-155: outbound + inbound A2A request shape with the
//!   D-317 signed AgentCard handle baked in.
//! - [`appcontainer`] — D-157: Windows `AppContainer` + `Job
//!   Objects` sandbox profile selected by `harness doctor sandbox`.

pub mod a2a;
pub mod appcontainer;
pub mod managed;

pub use a2a::{A2aDirection, A2aRequest, AgentCardHandle};
pub use appcontainer::{AppContainerProfile, JobObjectLimits, default_profile};
pub use managed::{
    ManagedAgent, ManagedKind, ScheduledAgent, validate_cron, validate_managed, validate_scheduled,
};
