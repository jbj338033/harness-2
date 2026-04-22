// IMPLEMENTS: D-312
//! Post-hoc Explanation Projection — given a target event id, reconstructs
//! the chain of reasoning that led to it. D-265 said "natural language",
//! D-271 said "structured JSON" — D-312 says emit both, with one natural
//! summary plus the typed list of supporting events for tooling.
//!
//! The function takes a flat event log (id, kind, when, summary, optional
//! causation_id) so it stays decoupled from `harness-storage`.

use crate::conversation_url::{LineRange, build_url};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExplainEvent {
    pub id: String,
    pub kind: String,
    pub summary: String,
    pub created_at: i64,
    /// id of the event that caused this one (if known) — lets us walk back
    /// through Plan → Think → Act chains without re-implementing the events
    /// table query.
    pub causation_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EventRef {
    pub id: String,
    pub kind: String,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Explanation {
    pub question: String,
    pub answer_natural: String,
    pub answer_structured: Vec<EventRef>,
    pub conversation_url: String,
}

/// Walk back from `target_event_id` through `causation_id` links and emit
/// both a sentence-level summary (D-265) and the structured event chain
/// (D-271). Cycles are broken by a visited-set so a malformed trace can't
/// loop the explainer.
#[must_use]
pub fn explain_decision(
    events: &[ExplainEvent],
    target_event_id: &str,
    session_id: &str,
    code_pin: Option<(&Path, LineRange)>,
) -> Option<Explanation> {
    let by_id: HashMap<&str, &ExplainEvent> = events.iter().map(|e| (e.id.as_str(), e)).collect();
    let target = by_id.get(target_event_id).copied()?;
    let chain = walk_chain(target, &by_id);

    let answer_natural = render_natural(target, &chain);
    let answer_structured: Vec<EventRef> = chain
        .iter()
        .map(|e| EventRef {
            id: e.id.clone(),
            kind: e.kind.clone(),
            summary: e.summary.clone(),
        })
        .collect();

    let url = code_pin.map_or_else(
        || build_url(session_id, None, None),
        |(path, range)| build_url(session_id, Some(path), Some(range)),
    );

    Some(Explanation {
        question: format!("Why did event {target_event_id} happen?"),
        answer_natural,
        answer_structured,
        conversation_url: url,
    })
}

fn walk_chain<'a>(
    target: &'a ExplainEvent,
    by_id: &HashMap<&str, &'a ExplainEvent>,
) -> Vec<&'a ExplainEvent> {
    // Walk parents first — caller ends up with [oldest, … target].
    let mut parents: Vec<&ExplainEvent> = Vec::new();
    let mut current = target;
    let mut visited: std::collections::HashSet<String> = std::collections::HashSet::new();
    visited.insert(current.id.clone());
    while let Some(parent_id) = current.causation_id.as_deref() {
        if !visited.insert(parent_id.to_string()) {
            break;
        }
        let Some(parent) = by_id.get(parent_id).copied() else {
            break;
        };
        parents.push(parent);
        current = parent;
    }
    parents.reverse();
    parents.push(target);
    parents
}

fn render_natural(target: &ExplainEvent, chain: &[&ExplainEvent]) -> String {
    let prelude: Vec<String> = chain
        .iter()
        .filter(|e| e.id != target.id)
        .map(|e| format!("{} ({})", e.kind, e.summary))
        .collect();
    if prelude.is_empty() {
        format!(
            "{} happened because there was no recorded prior cause; the agent acted on the most recent user message alone.",
            target.kind
        )
    } else {
        format!(
            "{} happened after: {}. The trigger was {}.",
            target.kind,
            prelude.join(" → "),
            target.summary
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(id: &str, kind: &str, summary: &str, parent: Option<&str>, t: i64) -> ExplainEvent {
        ExplainEvent {
            id: id.into(),
            kind: kind.into(),
            summary: summary.into(),
            created_at: t,
            causation_id: parent.map(str::to_string),
        }
    }

    #[test]
    fn missing_target_returns_none() {
        let events = vec![ev("a", "perceive", "hello", None, 1)];
        assert!(explain_decision(&events, "missing", "s", None).is_none());
    }

    #[test]
    fn explanation_walks_causation_chain_back_to_root() {
        let events = vec![
            ev("p1", "perceive", "user asked", None, 1),
            ev("t1", "think", "decide to read file", Some("p1"), 2),
            ev("a1", "act", "fs.read src/lib.rs", Some("t1"), 3),
        ];
        let exp = explain_decision(&events, "a1", "sess", None).unwrap();
        assert_eq!(exp.answer_structured.len(), 3);
        assert_eq!(exp.answer_structured[0].kind, "perceive");
        assert_eq!(exp.answer_structured[2].kind, "act");
        assert!(exp.answer_natural.contains("fs.read src/lib.rs"));
        assert!(exp.answer_natural.contains("perceive"));
    }

    #[test]
    fn cycle_in_causation_does_not_loop_forever() {
        // Pathological: a → b → a
        let events = vec![
            ev("a", "act", "a", Some("b"), 1),
            ev("b", "think", "b", Some("a"), 2),
        ];
        let exp = explain_decision(&events, "a", "s", None).unwrap();
        assert!(exp.answer_structured.len() <= 2);
    }

    #[test]
    fn explanation_pins_a_code_range_in_url_when_provided() {
        let events = vec![ev("a", "act", "edited", None, 1)];
        let exp = explain_decision(
            &events,
            "a",
            "sess",
            Some((Path::new("src/lib.rs"), LineRange::new(10, 12))),
        )
        .unwrap();
        assert!(exp.conversation_url.contains("path=src/lib.rs"));
        assert!(exp.conversation_url.contains("range=10-12"));
    }

    #[test]
    fn explanation_url_falls_back_to_session_only() {
        let events = vec![ev("a", "act", "edited", None, 1)];
        let exp = explain_decision(&events, "a", "sess", None).unwrap();
        assert_eq!(exp.conversation_url, "harness://session/sess");
    }

    #[test]
    fn root_target_natural_says_no_prior_cause() {
        let events = vec![ev("only", "act", "first", None, 1)];
        let exp = explain_decision(&events, "only", "s", None).unwrap();
        assert!(exp.answer_natural.contains("no recorded prior cause"));
    }

    #[test]
    fn structured_chain_serialises_round_trip() {
        let events = vec![
            ev("p1", "perceive", "x", None, 1),
            ev("a1", "act", "y", Some("p1"), 2),
        ];
        let exp = explain_decision(&events, "a1", "s", None).unwrap();
        let s = serde_json::to_string(&exp).unwrap();
        let back: Explanation = serde_json::from_str(&s).unwrap();
        assert_eq!(back, exp);
    }
}
