// IMPLEMENTS: D-261
//! Pointer to the four native data-engineering tool crates. Catalogue
//! / semantic / quality / lineage are MCP-delegated.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DataToolCrate {
    pub crate_name: &'static str,
    pub primary_skill: &'static str,
    pub mcp_delegated: bool,
}

#[must_use]
pub fn registered_data_crates() -> &'static [DataToolCrate] {
    const ROW: &[DataToolCrate] = &[
        DataToolCrate {
            crate_name: "harness-tools-sql",
            primary_skill: "query/explain/optimise",
            mcp_delegated: false,
        },
        DataToolCrate {
            crate_name: "harness-tools-dataframe",
            primary_skill: "wrangle/profile/plot",
            mcp_delegated: false,
        },
        DataToolCrate {
            crate_name: "harness-tools-notebook",
            primary_skill: "cell run / kernel mgmt",
            mcp_delegated: false,
        },
        DataToolCrate {
            crate_name: "harness-tools-pipeline",
            primary_skill: "DAG plan / dry-run",
            mcp_delegated: false,
        },
    ];
    ROW
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn four_native_data_crates() {
        assert_eq!(registered_data_crates().len(), 4);
    }

    #[test]
    fn none_are_mcp_delegated() {
        assert!(registered_data_crates().iter().all(|c| !c.mcp_delegated));
    }
}
