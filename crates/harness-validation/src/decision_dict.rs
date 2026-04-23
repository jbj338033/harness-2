// IMPLEMENTS: D-153
//! Decision dictionary for a wave of parallel Workers. Main writes
//! the dictionary before fan-out; every Worker pins it into its
//! system prompt; if a Worker hits a question the dictionary doesn't
//! answer, it must Speak `DecisionNeeded` so Main can revise. The
//! revision lands at the same memory page so the next Worker pull
//! sees it. Cognition's Flappy Bird parallel-worker incident is the
//! cautionary tale.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use thiserror::Error;

pub const MEMORY_PAGE_KEY_PREFIX: &str = "wave:";

#[must_use]
pub fn page_key(correlation_id: &str) -> String {
    format!("{MEMORY_PAGE_KEY_PREFIX}{correlation_id}:decisions")
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecisionEntry {
    pub question: String,
    pub answer: String,
    pub revision: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecisionDictionary {
    pub correlation_id: String,
    pub entries: BTreeMap<String, DecisionEntry>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum DecisionDictionaryError {
    #[error("decision dictionary missing correlation_id")]
    MissingCorrelationId,
    #[error("revision for {0} would not advance (current {current}, incoming {incoming})", current = .1, incoming = .2)]
    StaleRevision(String, u32, u32),
}

impl DecisionDictionary {
    pub fn new(correlation_id: impl Into<String>) -> Result<Self, DecisionDictionaryError> {
        let correlation_id = correlation_id.into();
        if correlation_id.trim().is_empty() {
            return Err(DecisionDictionaryError::MissingCorrelationId);
        }
        Ok(Self {
            correlation_id,
            entries: BTreeMap::new(),
        })
    }
}

/// Apply a revision in-place. Revision number must strictly increase
/// per question — older revisions are refused so an out-of-order
/// Worker reply can't roll back the dictionary.
pub fn apply_revision(
    dict: &mut DecisionDictionary,
    entry: DecisionEntry,
) -> Result<(), DecisionDictionaryError> {
    if let Some(existing) = dict.entries.get(&entry.question)
        && entry.revision <= existing.revision
    {
        return Err(DecisionDictionaryError::StaleRevision(
            entry.question.clone(),
            existing.revision,
            entry.revision,
        ));
    }
    dict.entries.insert(entry.question.clone(), entry);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(q: &str, a: &str, rev: u32) -> DecisionEntry {
        DecisionEntry {
            question: q.into(),
            answer: a.into(),
            revision: rev,
        }
    }

    #[test]
    fn page_key_layout_is_wave_prefixed() {
        assert_eq!(page_key("abc"), "wave:abc:decisions");
    }

    #[test]
    fn empty_correlation_id_rejected() {
        assert!(matches!(
            DecisionDictionary::new("  "),
            Err(DecisionDictionaryError::MissingCorrelationId)
        ));
    }

    #[test]
    fn first_revision_inserts() {
        let mut d = DecisionDictionary::new("c1").unwrap();
        apply_revision(&mut d, entry("color?", "blue", 1)).unwrap();
        assert_eq!(d.entries["color?"].answer, "blue");
    }

    #[test]
    fn newer_revision_overwrites() {
        let mut d = DecisionDictionary::new("c1").unwrap();
        apply_revision(&mut d, entry("color?", "blue", 1)).unwrap();
        apply_revision(&mut d, entry("color?", "red", 2)).unwrap();
        assert_eq!(d.entries["color?"].answer, "red");
    }

    #[test]
    fn stale_revision_refused() {
        let mut d = DecisionDictionary::new("c1").unwrap();
        apply_revision(&mut d, entry("color?", "blue", 5)).unwrap();
        let r = apply_revision(&mut d, entry("color?", "red", 4));
        assert!(matches!(r, Err(DecisionDictionaryError::StaleRevision(..))));
    }
}
