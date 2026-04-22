// IMPLEMENTS: D-272
//! Friendly MCP belt — the four non-developer integrations that ship
//! with a curated user-facing name and a one-line description.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FriendlyName {
    Mail,
    Calendar,
    Docs,
    Slack,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BeltAdapter {
    pub friendly: FriendlyName,
    pub mcp_server: &'static str,
    pub one_line: &'static str,
}

#[must_use]
pub fn registered_belt() -> &'static [BeltAdapter] {
    const ROW: &[BeltAdapter] = &[
        BeltAdapter {
            friendly: FriendlyName::Mail,
            mcp_server: "mcp://mail",
            one_line: "Read and draft email — never sends without you tapping send.",
        },
        BeltAdapter {
            friendly: FriendlyName::Calendar,
            mcp_server: "mcp://calendar",
            one_line: "See your day, propose meetings, ask before booking.",
        },
        BeltAdapter {
            friendly: FriendlyName::Docs,
            mcp_server: "mcp://docs",
            one_line: "Pull up a doc by name, suggest edits, never overwrite without OK.",
        },
        BeltAdapter {
            friendly: FriendlyName::Slack,
            mcp_server: "mcp://slack",
            one_line: "Catch up a channel, draft a reply — sending is always your tap.",
        },
    ];
    ROW
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn four_belt_adapters() {
        assert_eq!(registered_belt().len(), 4);
    }

    #[test]
    fn each_adapter_has_one_line() {
        assert!(
            registered_belt()
                .iter()
                .all(|a| !a.one_line.trim().is_empty())
        );
    }
}
