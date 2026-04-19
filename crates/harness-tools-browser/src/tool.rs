use crate::cdp::{CdpClient, CdpError};
use crate::snapshot::{AxNode, Snapshot, render_snapshot};
use async_trait::async_trait;
use harness_tools::{Tool, ToolContext, ToolError, ToolOutput};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct BrowserTool {
    endpoint: String,
    session: Arc<Mutex<Option<SessionState>>>,
}

struct SessionState {
    client: Arc<CdpClient>,
    last_refs: HashMap<String, i64>,
}

impl BrowserTool {
    #[must_use]
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            session: Arc::new(Mutex::new(None)),
        }
    }

    async fn ensure_session(&self) -> Result<Arc<CdpClient>, ToolError> {
        let mut guard = self.session.lock().await;
        if let Some(s) = guard.as_ref() {
            return Ok(s.client.clone());
        }
        let client = CdpClient::connect_new_page(&self.endpoint)
            .await
            .map_err(|e| ToolError::Other(format!("browser connect: {e}")))?;
        client
            .send("Page.enable", json!({}))
            .await
            .map_err(|e| cdp_to_tool(&e))?;
        client
            .send("Accessibility.enable", json!({}))
            .await
            .map_err(|e| cdp_to_tool(&e))?;
        let arc = Arc::new(client);
        *guard = Some(SessionState {
            client: arc.clone(),
            last_refs: HashMap::new(),
        });
        Ok(arc)
    }

    async fn record_refs(&self, refs: HashMap<String, i64>) {
        let mut guard = self.session.lock().await;
        if let Some(s) = guard.as_mut() {
            s.last_refs = refs;
        }
    }

    async fn resolve_ref(&self, r: &str) -> Option<i64> {
        self.session
            .lock()
            .await
            .as_ref()
            .and_then(|s| s.last_refs.get(r).copied())
    }
}

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
enum Input {
    Navigate { url: String },
    Snapshot,
    Screenshot,
    Evaluate { expression: String },
    Click { r#ref: String },
    Fill { r#ref: String, text: String },
}

#[async_trait]
impl Tool for BrowserTool {
    fn name(&self) -> &'static str {
        "browser"
    }

    fn description(&self) -> &'static str {
        "Drive a running Chrome instance via the CDP endpoint.\n\
         USE: inspect a live page, click/fill/snapshot for agent-driven workflows.\n\
         DO NOT USE: for static-site scraping — use `web_fetch` instead."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["action"],
            "oneOf": [
                {"required": ["action", "url"],
                 "properties": {"action": {"const": "navigate"}, "url": {"type": "string"}}},
                {"required": ["action"],
                 "properties": {"action": {"const": "snapshot"}}},
                {"required": ["action"],
                 "properties": {"action": {"const": "screenshot"}}},
                {"required": ["action", "expression"],
                 "properties": {"action": {"const": "evaluate"}, "expression": {"type": "string"}}},
                {"required": ["action", "ref"],
                 "properties": {"action": {"const": "click"}, "ref": {"type": "string"}}},
                {"required": ["action", "ref", "text"],
                 "properties": {"action": {"const": "fill"}, "ref": {"type": "string"}, "text": {"type": "string"}}}
            ]
        })
    }

    async fn execute(&self, input: Value, _ctx: &ToolContext) -> Result<ToolOutput, ToolError> {
        let parsed: Input =
            serde_json::from_value(input).map_err(|e| ToolError::Input(e.to_string()))?;
        let client = self.ensure_session().await?;

        match parsed {
            Input::Navigate { url } => navigate(&client, url).await,
            Input::Snapshot => self.snapshot(&client).await,
            Input::Screenshot => screenshot(&client).await,
            Input::Evaluate { expression } => evaluate(&client, expression).await,
            Input::Click { r#ref } => self.click(&client, r#ref).await,
            Input::Fill { r#ref, text } => self.fill(&client, r#ref, text).await,
        }
    }
}

impl BrowserTool {
    async fn snapshot(&self, client: &CdpClient) -> Result<ToolOutput, ToolError> {
        let result = client
            .send("Accessibility.getFullAXTree", json!({}))
            .await
            .map_err(|e| cdp_to_tool(&e))?;
        let nodes: Vec<AxNode> = serde_json::from_value(
            result
                .get("nodes")
                .cloned()
                .unwrap_or(Value::Array(Vec::new())),
        )
        .map_err(|e| ToolError::Other(e.to_string()))?;
        let snap: Snapshot = render_snapshot(&nodes);
        self.record_refs(snap.refs.clone()).await;
        Ok(ToolOutput::ok(snap.rendered).with_metadata(json!({ "refs": snap.refs.len() })))
    }

    async fn click(&self, client: &CdpClient, r: String) -> Result<ToolOutput, ToolError> {
        let Some(node_id) = self.resolve_ref(&r).await else {
            return Err(ToolError::Input(format!("unknown ref: {r}")));
        };
        let object_id = resolve_object_id(client, node_id).await?;
        client
            .send(
                "Runtime.callFunctionOn",
                json!({
                    "functionDeclaration": "function() { this.click(); }",
                    "objectId": object_id,
                    "awaitPromise": true,
                }),
            )
            .await
            .map_err(|e| cdp_to_tool(&e))?;
        Ok(ToolOutput::ok(format!("clicked ref {r}")))
    }

    async fn fill(
        &self,
        client: &CdpClient,
        r: String,
        text: String,
    ) -> Result<ToolOutput, ToolError> {
        let Some(node_id) = self.resolve_ref(&r).await else {
            return Err(ToolError::Input(format!("unknown ref: {r}")));
        };
        let object_id = resolve_object_id(client, node_id).await?;
        client
            .send(
                "Runtime.callFunctionOn",
                json!({
                    "functionDeclaration":
                        "function(t) { this.focus(); this.value = t; this.dispatchEvent(new Event('input', {bubbles: true})); this.dispatchEvent(new Event('change', {bubbles: true})); }",
                    "objectId": object_id,
                    "arguments": [{"value": text}],
                    "awaitPromise": true,
                }),
            )
            .await
            .map_err(|e| cdp_to_tool(&e))?;
        Ok(ToolOutput::ok(format!("filled ref {r}")))
    }
}

async fn navigate(client: &CdpClient, url: String) -> Result<ToolOutput, ToolError> {
    client
        .send("Page.navigate", json!({ "url": url }))
        .await
        .map_err(|e| cdp_to_tool(&e))?;
    Ok(ToolOutput::ok(format!("navigated to {url}")))
}

async fn screenshot(client: &CdpClient) -> Result<ToolOutput, ToolError> {
    let result = client
        .send("Page.captureScreenshot", json!({ "format": "png" }))
        .await
        .map_err(|e| cdp_to_tool(&e))?;
    let data = result
        .get("data")
        .and_then(Value::as_str)
        .ok_or_else(|| ToolError::Other("no screenshot data".into()))?;
    Ok(ToolOutput::ok("captured screenshot".to_string())
        .with_metadata(json!({ "base64_png": data })))
}

async fn evaluate(client: &CdpClient, expression: String) -> Result<ToolOutput, ToolError> {
    let result = client
        .send(
            "Runtime.evaluate",
            json!({
                "expression": expression,
                "returnByValue": true,
                "awaitPromise": true,
            }),
        )
        .await
        .map_err(|e| cdp_to_tool(&e))?;
    let value = result
        .get("result")
        .and_then(|r| r.get("value"))
        .cloned()
        .unwrap_or(Value::Null);
    Ok(ToolOutput::ok(value.to_string()).with_metadata(value))
}

async fn resolve_object_id(client: &CdpClient, node_id: i64) -> Result<String, ToolError> {
    let resolved = client
        .send("DOM.resolveNode", json!({ "backendNodeId": node_id }))
        .await
        .map_err(|e| cdp_to_tool(&e))?;
    resolved
        .get("object")
        .and_then(|o| o.get("objectId"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| ToolError::Other("no objectId".into()))
}

fn cdp_to_tool(e: &CdpError) -> ToolError {
    ToolError::Other(format!("cdp: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_schema_declares_oneof() {
        let t = BrowserTool::new("http://localhost:9222");
        let s = t.input_schema();
        assert!(s["oneOf"].is_array());
    }

    #[tokio::test]
    async fn execute_rejects_unknown_shape() {
        let t = BrowserTool::new("http://localhost:9222");
        let ctx = ToolContext::test("/tmp");
        let err = t
            .execute(json!({"action": "unknown"}), &ctx)
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::Input(_)));
    }

    #[tokio::test]
    async fn execute_reports_connect_failure() {
        let t = BrowserTool::new("http://127.0.0.1:1");
        let ctx = ToolContext::test("/tmp");
        let err = t
            .execute(
                json!({"action": "navigate", "url": "https://example.com"}),
                &ctx,
            )
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::Other(_)));
    }
}
