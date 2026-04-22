// IMPLEMENTS: D-265
//! Text-to-SQL semantic layer pointer. The layer itself is supplied
//! over MCP — Cortex Analyst, dbt Copilot, or a custom adapter.
//! Re-implementing it inside Harness was explicitly rejected.

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SemanticAdapterKind {
    CortexAnalyst,
    DbtCopilot,
    Custom,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SemanticAdapter {
    pub kind: SemanticAdapterKind,
    pub mcp_server_uri: String,
    pub model_layer_name: String,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SemanticAdapterError {
    #[error("MCP server URI must not be empty")]
    EmptyUri,
    #[error("model layer name must not be empty")]
    EmptyLayer,
}

pub fn register_semantic_adapter(
    adapter: SemanticAdapter,
) -> Result<SemanticAdapter, SemanticAdapterError> {
    if adapter.mcp_server_uri.trim().is_empty() {
        return Err(SemanticAdapterError::EmptyUri);
    }
    if adapter.model_layer_name.trim().is_empty() {
        return Err(SemanticAdapterError::EmptyLayer);
    }
    Ok(adapter)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cortex_adapter_registered() {
        let a = SemanticAdapter {
            kind: SemanticAdapterKind::CortexAnalyst,
            mcp_server_uri: "https://example.com/mcp".into(),
            model_layer_name: "warehouse.semantic".into(),
        };
        assert!(register_semantic_adapter(a).is_ok());
    }

    #[test]
    fn empty_uri_rejected() {
        let a = SemanticAdapter {
            kind: SemanticAdapterKind::Custom,
            mcp_server_uri: "  ".into(),
            model_layer_name: "x".into(),
        };
        assert_eq!(
            register_semantic_adapter(a),
            Err(SemanticAdapterError::EmptyUri)
        );
    }
}
