// IMPLEMENTS: D-360
//! Clause-library aware DOCX redline planner. We compute the *plan*
//! (which clauses to insert / replace / strike); the actual DOCX
//! serialisation lives in `harness-tools-legal-redline` so this crate
//! stays pure-data.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClauseLibrary {
    pub clauses: BTreeMap<String, String>,
}

impl ClauseLibrary {
    pub fn insert(&mut self, key: impl Into<String>, body: impl Into<String>) {
        self.clauses.insert(key.into(), body.into());
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum RedlineOp {
    InsertClause { after_paragraph: usize, key: String },
    ReplaceClause { paragraph: usize, key: String },
    StrikeParagraph { paragraph: usize },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RedlinePlan {
    pub document_id: String,
    pub ops: Vec<RedlineOp>,
}

impl RedlinePlan {
    /// Resolve every `key` against the library — returns the index of
    /// the first op whose clause is missing.
    pub fn validate(&self, library: &ClauseLibrary) -> Result<(), usize> {
        for (i, op) in self.ops.iter().enumerate() {
            let key = match op {
                RedlineOp::InsertClause { key, .. } | RedlineOp::ReplaceClause { key, .. } => {
                    Some(key)
                }
                RedlineOp::StrikeParagraph { .. } => None,
            };
            if let Some(k) = key
                && !library.clauses.contains_key(k)
            {
                return Err(i);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lib() -> ClauseLibrary {
        let mut l = ClauseLibrary::default();
        l.insert("indemnity", "Each party shall indemnify…");
        l.insert("nda", "Confidential information…");
        l
    }

    #[test]
    fn plan_validates_when_all_keys_resolve() {
        let p = RedlinePlan {
            document_id: "doc-1".into(),
            ops: vec![
                RedlineOp::InsertClause {
                    after_paragraph: 4,
                    key: "indemnity".into(),
                },
                RedlineOp::StrikeParagraph { paragraph: 7 },
            ],
        };
        assert!(p.validate(&lib()).is_ok());
    }

    #[test]
    fn missing_clause_key_fails_validation() {
        let p = RedlinePlan {
            document_id: "doc-1".into(),
            ops: vec![RedlineOp::ReplaceClause {
                paragraph: 1,
                key: "noncompete".into(),
            }],
        };
        assert_eq!(p.validate(&lib()), Err(0));
    }
}
