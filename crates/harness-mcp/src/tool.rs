use crate::client::{ManagedServer, ServerTool};
use async_trait::async_trait;
use harness_tools::{Tool, ToolContext, ToolError, ToolOutput};
use serde_json::{Value, json};
use std::sync::Arc;

pub struct McpTool {
    name: String,
    description: String,
    input_schema: Value,
    remote_name: String,
    server: Arc<ManagedServer>,
}

impl McpTool {
    #[must_use]
    pub fn new(prefix: &str, remote: ServerTool, server: Arc<ManagedServer>) -> Self {
        let name = format!("{prefix}::{}", remote.name);
        let description = if remote.description.is_empty() {
            format!("MCP tool {} from server {}", remote.name, prefix)
        } else {
            remote.description
        };
        let input_schema = if remote.input_schema.is_null() {
            json!({"type": "object"})
        } else {
            remote.input_schema
        };
        Self {
            name,
            description,
            input_schema,
            remote_name: remote.name,
            server,
        }
    }
}

#[async_trait]
impl Tool for McpTool {
    fn name(&self) -> &str {
        &self.name
    }
    fn description(&self) -> &str {
        &self.description
    }
    fn input_schema(&self) -> Value {
        self.input_schema.clone()
    }
    async fn execute(&self, input: Value, _ctx: &ToolContext) -> Result<ToolOutput, ToolError> {
        let result = self
            .server
            .call_tool(&self.remote_name, input)
            .await
            .map_err(|e| ToolError::Other(e.to_string()))?;

        let content = result
            .get("content")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let mut text = String::new();
        for block in content {
            if let Some(t) = block.get("text").and_then(Value::as_str) {
                if !text.is_empty() {
                    text.push('\n');
                }
                text.push_str(t);
            }
        }
        let is_error = result
            .get("isError")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let out = if is_error {
            ToolOutput::err(text)
        } else {
            ToolOutput::ok(text)
        };
        Ok(out.with_metadata(result))
    }
}
