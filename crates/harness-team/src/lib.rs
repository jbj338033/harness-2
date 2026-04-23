// IMPLEMENTS: D-240, D-241, D-242, D-243, D-244, D-245, D-246, D-247, D-248, D-249, D-250
//! Team-agent primitives.
//!
//! - [`actor_ref`] — `ActorRef` enum used by `Speak.to` (D-240) and
//!   `Broadcast` (D-248).
//! - [`mailbox`] — D-246 mailbox projection row.
//! - [`handoff`] — D-241 Handoff event with `input_filter` and
//!   `on_handoff` hook + D-250 swarm-refusal gate (the only allowed
//!   handoff path is A → Main → B).
//! - [`input_filter`] — D-242 subagent context filter
//!   (`Full / Summary(max_tokens) / Slice(event_kinds)`).
//! - [`parallelism`] — D-243 `Sequential / Wave` capability + D-247
//!   `WaveCoordinator` plan struct.
//! - [`task`] — D-244 task projection row.
//! - [`topic`] — D-245 `Contract(kind=Topic)` append-only pub-sub.
//! - [`presets`] — D-249 nine canonical role presets.

pub mod actor_ref;
pub mod handoff;
pub mod input_filter;
pub mod mailbox;
pub mod parallelism;
pub mod presets;
pub mod task;
pub mod topic;

pub use actor_ref::{ActorKind, ActorRef, CorrelationId};
pub use handoff::{HandoffEvent, HandoffPath, HandoffRefusal, evaluate_handoff_path};
pub use input_filter::SubagentInputFilter;
pub use mailbox::MailboxRow;
pub use parallelism::{DisjointCheck, ParallelismCapability, WaveCoordinatorPlan, plan_wave};
pub use presets::{TeamRolePreset, all_team_role_presets};
pub use task::{TaskRow, TaskStatus};
pub use topic::{TopicContract, TopicMessage};
