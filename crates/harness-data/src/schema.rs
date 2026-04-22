// IMPLEMENTS: D-263
//! Schema projection — the subset of catalog metadata the model
//! actually needs to write correct SQL: tables, columns, foreign
//! keys, dimensions, measures. Storing more (DDL, comments, sample
//! rows) measurably hurts text-to-SQL accuracy on Spider 2.0 and
//! BIRD-SQL — so we cap the projection here.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SchemaRole {
    Dimension,
    Measure,
    Identifier,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColumnRow {
    pub name: String,
    pub sql_type: String,
    pub role: SchemaRole,
    pub nullable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableRow {
    pub name: String,
    pub columns: Vec<ColumnRow>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForeignKeyRow {
    pub from_table: String,
    pub from_column: String,
    pub to_table: String,
    pub to_column: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchemaProjection {
    pub tables: BTreeMap<String, TableRow>,
    pub fks: Vec<ForeignKeyRow>,
}

impl SchemaProjection {
    #[must_use]
    pub fn dimensions_for(&self, table: &str) -> Vec<&ColumnRow> {
        self.tables
            .get(table)
            .map(|t| {
                t.columns
                    .iter()
                    .filter(|c| matches!(c.role, SchemaRole::Dimension))
                    .collect()
            })
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn projection() -> SchemaProjection {
        let mut tables = BTreeMap::new();
        tables.insert(
            "orders".into(),
            TableRow {
                name: "orders".into(),
                columns: vec![
                    ColumnRow {
                        name: "region".into(),
                        sql_type: "text".into(),
                        role: SchemaRole::Dimension,
                        nullable: false,
                    },
                    ColumnRow {
                        name: "amount".into(),
                        sql_type: "numeric".into(),
                        role: SchemaRole::Measure,
                        nullable: false,
                    },
                ],
            },
        );
        SchemaProjection {
            tables,
            fks: vec![ForeignKeyRow {
                from_table: "orders".into(),
                from_column: "customer_id".into(),
                to_table: "customers".into(),
                to_column: "id".into(),
            }],
        }
    }

    #[test]
    fn dimensions_filtered() {
        let p = projection();
        let dims = p.dimensions_for("orders");
        assert_eq!(dims.len(), 1);
        assert_eq!(dims[0].name, "region");
    }

    #[test]
    fn unknown_table_returns_empty() {
        let p = projection();
        assert!(p.dimensions_for("nope").is_empty());
    }
}
