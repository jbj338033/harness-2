// IMPLEMENTS: D-353
//! CodeQL + QLCoder query metadata. The CI step compiles a curated
//! pack of queries (rust-security + rust-correctness + the
//! Harness-specific event-store invariants) and surfaces hits to the
//! release gate.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CodeqlSeverity {
    Note,
    Warning,
    Error,
    Critical,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodeqlQuery {
    pub id: &'static str,
    pub severity: CodeqlSeverity,
    pub qlcoder_assisted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeqlPack {
    pub queries: &'static [CodeqlQuery],
}

#[must_use]
pub fn registered_codeql_pack() -> CodeqlPack {
    const PACK: &[CodeqlQuery] = &[
        CodeqlQuery {
            id: "rust/security/path-traversal",
            severity: CodeqlSeverity::Error,
            qlcoder_assisted: true,
        },
        CodeqlQuery {
            id: "rust/security/command-injection",
            severity: CodeqlSeverity::Critical,
            qlcoder_assisted: true,
        },
        CodeqlQuery {
            id: "rust/correctness/unwrap-on-none",
            severity: CodeqlSeverity::Warning,
            qlcoder_assisted: false,
        },
        CodeqlQuery {
            id: "harness/events/append-only",
            severity: CodeqlSeverity::Critical,
            qlcoder_assisted: false,
        },
        CodeqlQuery {
            id: "harness/storage/with-tx-required",
            severity: CodeqlSeverity::Error,
            qlcoder_assisted: false,
        },
    ];
    CodeqlPack { queries: PACK }
}

#[must_use]
pub fn codeql_query_count() -> usize {
    registered_codeql_pack().queries.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_has_five_queries() {
        assert_eq!(codeql_query_count(), 5);
    }

    #[test]
    fn append_only_query_is_critical() {
        let p = registered_codeql_pack();
        let q = p
            .queries
            .iter()
            .find(|q| q.id == "harness/events/append-only")
            .unwrap();
        assert_eq!(q.severity, CodeqlSeverity::Critical);
    }

    #[test]
    fn at_least_one_qlcoder_assisted() {
        let p = registered_codeql_pack();
        assert!(p.queries.iter().any(|q| q.qlcoder_assisted));
    }
}
