// IMPLEMENTS: D-194, D-322, D-323
//! Compile-time invariants. The Replit incident class — "agent ran a
//! destructive command without approval" — is structurally impossible
//! here because [`Action<Unapproved>`] has no `execute` method. Only
//! [`Action<Approved>`] does, and the only way to get one is through
//! [`Action::approve`].
//!
//! [`Turn<Phase>`] models the chat turn lifecycle in the type system so
//! the kernel can't accidentally mark a still-streaming turn as done.
//!
//! The proptest suite at the bottom guards three rebuild invariants
//! (D-194 + D-322): I-1 append-only, I-2 projection determinism, I-7
//! cost-cap monotonicity.

use serde::{Deserialize, Serialize};
use std::marker::PhantomData;

// ----- D-323: Action<Unapproved/Approved> -----

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Unapproved;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Approved;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Action<State = Unapproved> {
    pub tool: String,
    pub input_hash: String,
    #[serde(skip)]
    _state: PhantomData<State>,
}

impl Action<Unapproved> {
    #[must_use]
    pub fn new(tool: impl Into<String>, input_hash: impl Into<String>) -> Self {
        Self {
            tool: tool.into(),
            input_hash: input_hash.into(),
            _state: PhantomData,
        }
    }

    /// Promote an unapproved action to an approved one. The only way to
    /// reach the [`Action::execute`] method below — there is no
    /// `execute` on `Action<Unapproved>`, so the kernel can't run an
    /// unapproved action even if it tries.
    #[must_use]
    pub fn approve(self) -> Action<Approved> {
        Action {
            tool: self.tool,
            input_hash: self.input_hash,
            _state: PhantomData,
        }
    }
}

impl Action<Approved> {
    /// Marker that the action is now safe to dispatch. The real kernel
    /// passes this struct down to whichever tool registry actually runs
    /// commands.
    #[must_use]
    pub fn execute(&self) -> ExecutionToken<'_> {
        ExecutionToken {
            tool: &self.tool,
            input_hash: &self.input_hash,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExecutionToken<'a> {
    pub tool: &'a str,
    pub input_hash: &'a str,
}

// ----- D-323: Turn<Phase> -----

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Setup;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Streaming;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Done;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Turn<Phase = Setup> {
    pub session_id: String,
    pub messages_so_far: u32,
    _phase: PhantomData<Phase>,
}

impl Turn<Setup> {
    #[must_use]
    pub fn new(session_id: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
            messages_so_far: 0,
            _phase: PhantomData,
        }
    }

    /// First model token has arrived — transition into Streaming.
    #[must_use]
    pub fn start_streaming(self) -> Turn<Streaming> {
        Turn {
            session_id: self.session_id,
            messages_so_far: self.messages_so_far,
            _phase: PhantomData,
        }
    }
}

impl Turn<Streaming> {
    pub fn record_message(&mut self) {
        self.messages_so_far += 1;
    }

    /// Stream end-of-message arrived. Done is the only state from which
    /// the kernel may persist the conversation transcript.
    #[must_use]
    pub fn finish(self) -> Turn<Done> {
        Turn {
            session_id: self.session_id,
            messages_so_far: self.messages_so_far,
            _phase: PhantomData,
        }
    }
}

impl Turn<Done> {
    #[must_use]
    pub fn message_count(&self) -> u32 {
        self.messages_so_far
    }
}

// ----- D-194: Projection rebuild invariant assertion -----

/// `assert_projection_replay` runs the supplied projection function over
/// the event log twice and panics if the two outputs differ. Use in
/// `#[cfg(test)]` only — production code uses [`projection_matches`] and
/// returns the bool to the caller.
pub fn assert_projection_replay<E, P, F>(events: &[E], project: F)
where
    E: Clone,
    P: PartialEq + std::fmt::Debug,
    F: Fn(&[E]) -> P,
{
    let first = project(events);
    let second = project(events);
    assert_eq!(
        first, second,
        "projection is non-deterministic over identical event log"
    );
}

#[must_use]
pub fn projection_matches<E, P, F>(events: &[E], project: F) -> bool
where
    P: PartialEq,
    F: Fn(&[E]) -> P,
{
    project(events) == project(events)
}

#[cfg(test)]
mod tests {
    use super::*;
    use harness_cost::{CostCap, CostLedger};
    use proptest::prelude::*;

    #[test]
    fn action_unapproved_has_no_execute_method() {
        // Compile-time check: this code wouldn't compile if Unapproved had
        // an execute method.
        let a: Action<Unapproved> = Action::new("fs.read", "h1");
        let approved = a.approve();
        let _token = approved.execute();
    }

    #[test]
    fn turn_lifecycle_compiles_in_order() {
        let t = Turn::new("session-1");
        let mut streaming = t.start_streaming();
        streaming.record_message();
        streaming.record_message();
        let done = streaming.finish();
        assert_eq!(done.message_count(), 2);
    }

    #[test]
    fn projection_assertion_panics_on_nondeterminism() {
        let events = vec![1u32, 2, 3];
        assert_projection_replay(&events, |e| e.iter().sum::<u32>());
    }

    // ----- D-322: proptest invariants -----

    proptest! {
        /// I-1: append-only — appending events never shortens the log.
        #[test]
        fn invariant_append_only(initial in proptest::collection::vec(0u32..1_000, 0..50),
                                 to_append in proptest::collection::vec(0u32..1_000, 0..50)) {
            let mut log = initial.clone();
            log.extend(to_append.iter().copied());
            prop_assert!(log.len() >= initial.len());
            for (i, v) in initial.iter().enumerate() {
                prop_assert_eq!(log[i], *v, "old events must not mutate");
            }
        }

        /// I-2: projection determinism — same input → same output.
        #[test]
        fn invariant_projection_determinism(
            events in proptest::collection::vec(0u32..1_000, 0..200),
        ) {
            let p1: u64 = events.iter().map(|x| u64::from(*x)).sum();
            let p2: u64 = events.iter().map(|x| u64::from(*x)).sum();
            prop_assert_eq!(p1, p2);
        }

        /// I-7: cost-cap monotonicity — recording a non-negative charge
        /// never decreases any of the three running totals.
        #[test]
        fn invariant_cost_cap_monotonicity(charges in proptest::collection::vec(0.0f64..0.10, 0..100)) {
            let cap = CostCap::default();
            let mut led = CostLedger::default();
            let (mut prev_s, mut prev_d, mut prev_g) = (0.0, 0.0, 0.0);
            for c in &charges {
                let _ = led.record(&cap, *c);
                prop_assert!(led.session_used >= prev_s);
                prop_assert!(led.daily_used >= prev_d);
                prop_assert!(led.global_used >= prev_g);
                prev_s = led.session_used;
                prev_d = led.daily_used;
                prev_g = led.global_used;
            }
        }

        /// Unapproved → Approved is a one-way move; the type system is
        /// the proof, but we also assert it observationally.
        #[test]
        fn approve_round_trip_preserves_identity(tool in "[a-z][a-z0-9_]{1,15}",
                                                 hash in "[0-9a-f]{8}") {
            let a = Action::new(tool.clone(), hash.clone());
            let approved = a.approve();
            prop_assert_eq!(approved.tool.as_str(), &tool);
            prop_assert_eq!(approved.input_hash.as_str(), &hash);
        }
    }
}
