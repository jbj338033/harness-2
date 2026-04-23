// IMPLEMENTS: D-241, D-250
//! Handoff event (control transfer) + swarm-refusal gate.
//!
//! The only allowed handoff path is `A → Main → B` (D-250). Direct
//! peer-to-peer handoff — OpenAI Swarm–style — is refused because it
//! violates the single-context invariant Cognition documented.

use crate::actor_ref::ActorRef;
use crate::input_filter::SubagentInputFilter;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HookRef(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HandoffEvent {
    pub from: ActorRef,
    pub to: ActorRef,
    pub input_filter: SubagentInputFilter,
    pub on_handoff: Option<HookRef>,
    pub reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HandoffPath {
    /// Allowed — `A → Main`, `Main → B`, or `A → Main → B` (two events).
    ViaMain,
    /// Refused — peer-to-peer skips the orchestrator.
    DirectPeerToPeer,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum HandoffRefusal {
    #[error("direct peer-to-peer handoff refused — handoff must route through Main (D-250)")]
    SwarmRefused,
}

#[must_use]
pub fn evaluate_handoff_path(event: &HandoffEvent) -> HandoffPath {
    match (&event.from, &event.to) {
        (ActorRef::Main, _) | (_, ActorRef::Main) => HandoffPath::ViaMain,
        _ => HandoffPath::DirectPeerToPeer,
    }
}

pub fn gate_handoff(event: &HandoffEvent) -> Result<(), HandoffRefusal> {
    match evaluate_handoff_path(event) {
        HandoffPath::ViaMain => Ok(()),
        HandoffPath::DirectPeerToPeer => Err(HandoffRefusal::SwarmRefused),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(from: ActorRef, to: ActorRef) -> HandoffEvent {
        HandoffEvent {
            from,
            to,
            input_filter: SubagentInputFilter::Full,
            on_handoff: None,
            reason: "test".into(),
        }
    }

    #[test]
    fn main_to_subagent_is_via_main() {
        let e = event(ActorRef::Main, ActorRef::Subagent { id: "code".into() });
        assert_eq!(evaluate_handoff_path(&e), HandoffPath::ViaMain);
        assert!(gate_handoff(&e).is_ok());
    }

    #[test]
    fn subagent_to_main_is_via_main() {
        let e = event(ActorRef::Subagent { id: "code".into() }, ActorRef::Main);
        assert_eq!(evaluate_handoff_path(&e), HandoffPath::ViaMain);
    }

    #[test]
    fn subagent_to_subagent_refused() {
        let e = event(
            ActorRef::Subagent { id: "a".into() },
            ActorRef::Subagent { id: "b".into() },
        );
        assert_eq!(evaluate_handoff_path(&e), HandoffPath::DirectPeerToPeer);
        assert_eq!(gate_handoff(&e), Err(HandoffRefusal::SwarmRefused));
    }
}
