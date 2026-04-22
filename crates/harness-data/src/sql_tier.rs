// IMPLEMENTS: D-262
//! 3-tier DB sandbox + lightweight static danger detector. The full
//! `sqlparser-rs` AST-level check lives in `harness-tools-sql`; this
//! module provides the keyword pre-filter every caller can run cheaply
//! before reaching the parser.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SqlSandboxTier {
    /// Read-only against production. Refuse anything mutating.
    ReadOnlyProd,
    /// Staging — DDL allowed; destructive DROP / TRUNCATE refused.
    Staging,
    /// Throwaway sandbox database — anything goes.
    Sandbox,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SqlStaticDanger {
    /// Pure SELECT / WITH … SELECT.
    None,
    /// INSERT / UPDATE / MERGE — mutating but recoverable in
    /// staging.
    Mutating,
    /// DROP / TRUNCATE / DELETE without WHERE — destructive.
    Destructive,
    /// Multi-statement query — refused outright (lets the LLM smuggle
    /// a destructive trailing statement past a parser-only check).
    MultiStatement,
}

#[must_use]
pub fn classify_sql(sql: &str) -> SqlStaticDanger {
    let lower = sql.to_ascii_lowercase();
    if has_unquoted_semicolon(&lower) {
        return SqlStaticDanger::MultiStatement;
    }
    if has_keyword(&lower, &["drop ", "truncate "]) {
        return SqlStaticDanger::Destructive;
    }
    if has_keyword(&lower, &["delete "]) && !has_keyword(&lower, &[" where "]) {
        return SqlStaticDanger::Destructive;
    }
    if has_keyword(&lower, &["insert ", "update ", "merge ", "delete "]) {
        return SqlStaticDanger::Mutating;
    }
    SqlStaticDanger::None
}

fn has_keyword(sql: &str, keywords: &[&str]) -> bool {
    keywords.iter().any(|k| sql.contains(k))
}

fn has_unquoted_semicolon(sql: &str) -> bool {
    let mut in_single = false;
    let mut in_double = false;
    let mut prev = ' ';
    for c in sql.chars() {
        match c {
            '\'' if !in_double && prev != '\\' => in_single = !in_single,
            '"' if !in_single && prev != '\\' => in_double = !in_double,
            ';' if !in_single && !in_double => {
                let trimmed = sql.trim_end();
                if !sql.ends_with(';') || trimmed.len() > sql.find(';').unwrap_or(0) + 1 {
                    return true;
                }
            }
            _ => {}
        }
        prev = c;
    }
    false
}

impl SqlSandboxTier {
    /// True if the query may be executed at this tier.
    #[must_use]
    pub fn permits(self, danger: SqlStaticDanger) -> bool {
        match (self, danger) {
            (_, SqlStaticDanger::MultiStatement) => false,
            (SqlSandboxTier::ReadOnlyProd, SqlStaticDanger::None) => true,
            (SqlSandboxTier::ReadOnlyProd, _) => false,
            (SqlSandboxTier::Staging, SqlStaticDanger::Destructive) => false,
            (SqlSandboxTier::Staging, _) => true,
            (SqlSandboxTier::Sandbox, _) => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn select_is_safe() {
        assert_eq!(classify_sql("SELECT * FROM t"), SqlStaticDanger::None);
    }

    #[test]
    fn drop_is_destructive() {
        assert_eq!(
            classify_sql("DROP TABLE customers"),
            SqlStaticDanger::Destructive
        );
    }

    #[test]
    fn delete_without_where_is_destructive() {
        assert_eq!(
            classify_sql("DELETE FROM orders"),
            SqlStaticDanger::Destructive
        );
    }

    #[test]
    fn delete_with_where_is_mutating_only() {
        assert_eq!(
            classify_sql("DELETE FROM orders WHERE id = 1"),
            SqlStaticDanger::Mutating
        );
    }

    #[test]
    fn multi_statement_caught() {
        assert_eq!(
            classify_sql("SELECT 1; DROP TABLE t"),
            SqlStaticDanger::MultiStatement
        );
    }

    #[test]
    fn read_only_prod_blocks_anything_mutating() {
        assert!(SqlSandboxTier::ReadOnlyProd.permits(SqlStaticDanger::None));
        assert!(!SqlSandboxTier::ReadOnlyProd.permits(SqlStaticDanger::Mutating));
        assert!(!SqlSandboxTier::ReadOnlyProd.permits(SqlStaticDanger::Destructive));
    }

    #[test]
    fn staging_allows_mutating_blocks_destructive() {
        assert!(SqlSandboxTier::Staging.permits(SqlStaticDanger::Mutating));
        assert!(!SqlSandboxTier::Staging.permits(SqlStaticDanger::Destructive));
    }

    #[test]
    fn sandbox_allows_destructive_but_not_multi_statement() {
        assert!(SqlSandboxTier::Sandbox.permits(SqlStaticDanger::Destructive));
        assert!(!SqlSandboxTier::Sandbox.permits(SqlStaticDanger::MultiStatement));
    }
}
