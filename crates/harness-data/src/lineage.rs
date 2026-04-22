// IMPLEMENTS: D-267
//! Data lineage stretch — OpenLineage / DataHub MCP descriptors. We
//! ship the pointer here so a stretch-goal MCP server can be slotted
//! in without touching the data tools.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LineageProvider {
    OpenLineage,
    DataHub,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LineageStretchPointer {
    pub provider: LineageProvider,
    pub mcp_server_uri: String,
    pub enabled: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pointer_round_trips() {
        let p = LineageStretchPointer {
            provider: LineageProvider::OpenLineage,
            mcp_server_uri: "https://example.com/openlineage".into(),
            enabled: false,
        };
        let s = serde_json::to_string(&p).unwrap();
        let back: LineageStretchPointer = serde_json::from_str(&s).unwrap();
        assert_eq!(back, p);
        assert!(s.contains("open_lineage"));
    }

    #[test]
    fn stretch_disabled_by_default() {
        let p = LineageStretchPointer {
            provider: LineageProvider::DataHub,
            mcp_server_uri: "x".into(),
            enabled: false,
        };
        assert!(!p.enabled);
    }
}
