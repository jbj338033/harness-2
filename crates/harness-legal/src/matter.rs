// IMPLEMENTS: D-357
//! `Matter` primitive — every legal session is bound to one. Drives
//! jurisdiction selection, memory scoping, and the Chinese-wall list
//! of opposing-party identifiers that must never appear in retrieved
//! context.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct MatterId(pub String);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Jurisdiction {
    UsFed,
    UsCa,
    UsNy,
    Uk,
    Eu,
    Kr,
    Jp,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChineseWall {
    pub blocked_party_ids: BTreeSet<String>,
}

impl ChineseWall {
    pub fn block(&mut self, id: impl Into<String>) {
        self.blocked_party_ids.insert(id.into());
    }

    #[must_use]
    pub fn would_violate(&self, party_id: &str) -> bool {
        self.blocked_party_ids.contains(party_id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Matter {
    pub id: MatterId,
    pub jurisdiction: Jurisdiction,
    /// Memory scope — string keys that bound the matter's recall
    /// (typically a folder prefix in the memory store).
    pub memory_scopes: BTreeSet<String>,
    pub wall: ChineseWall,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum MatterScopeError {
    #[error("party {0} is on the matter's chinese wall — recall refused")]
    ChineseWallViolation(String),
    #[error("memory key {0} is outside this matter's scope")]
    OutOfScope(String),
}

impl Matter {
    pub fn new(id: impl Into<String>, jurisdiction: Jurisdiction) -> Self {
        Self {
            id: MatterId(id.into()),
            jurisdiction,
            memory_scopes: BTreeSet::new(),
            wall: ChineseWall::default(),
        }
    }

    pub fn check_recall(
        &self,
        memory_key: &str,
        party_ids: &[&str],
    ) -> Result<(), MatterScopeError> {
        for p in party_ids {
            if self.wall.would_violate(p) {
                return Err(MatterScopeError::ChineseWallViolation((*p).to_string()));
            }
        }
        if self.memory_scopes.is_empty() {
            return Ok(());
        }
        if self
            .memory_scopes
            .iter()
            .any(|prefix| memory_key.starts_with(prefix))
        {
            Ok(())
        } else {
            Err(MatterScopeError::OutOfScope(memory_key.to_string()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_memory_scope_is_unrestricted() {
        let m = Matter::new("m1", Jurisdiction::UsCa);
        assert!(m.check_recall("any/key", &[]).is_ok());
    }

    #[test]
    fn out_of_scope_memory_key_rejected() {
        let mut m = Matter::new("m1", Jurisdiction::UsCa);
        m.memory_scopes.insert("matter/m1/".into());
        assert!(matches!(
            m.check_recall("matter/m2/note", &[]),
            Err(MatterScopeError::OutOfScope(_))
        ));
    }

    #[test]
    fn chinese_wall_blocks_opposing_party() {
        let mut m = Matter::new("m1", Jurisdiction::UsFed);
        m.wall.block("opposing-party-id");
        let r = m.check_recall("any", &["opposing-party-id"]);
        assert!(matches!(r, Err(MatterScopeError::ChineseWallViolation(_))));
    }
}
