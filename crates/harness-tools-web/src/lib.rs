use async_trait::async_trait;
use harness_tools::{Tool, ToolContext, ToolError, ToolOutput};
use reqwest::Client;
use serde::Deserialize;
use serde_json::{Value, json};
use std::time::Duration;

const MAX_BYTES: usize = 512 * 1024;

pub struct WebFetchTool {
    client: Client,
}

impl Default for WebFetchTool {
    fn default() -> Self {
        Self::new()
    }
}

impl WebFetchTool {
    #[must_use]
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .expect("reqwest client"),
        }
    }

    #[must_use]
    pub fn with_client(client: Client) -> Self {
        Self { client }
    }
}

#[derive(Debug, Deserialize)]
struct FetchInput {
    url: String,
    #[serde(default)]
    accept: Option<String>,
}

#[async_trait]
impl Tool for WebFetchTool {
    fn name(&self) -> &'static str {
        "web_fetch"
    }

    fn description(&self) -> &'static str {
        "Fetch a URL over HTTP(S). Returns up to 512 KiB of the body as \
         text.\n\
         USE: reading documentation, fetching JSON APIs, small page scrapes.\n\
         DO NOT USE: binary downloads (use `bash` with `curl`); \
         authenticated endpoints (use an MCP server)."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["url"],
            "properties": {
                "url": { "type": "string", "format": "uri" },
                "accept": { "type": "string", "description": "Accept header override" }
            }
        })
    }

    async fn execute(&self, input: Value, _ctx: &ToolContext) -> Result<ToolOutput, ToolError> {
        let args: FetchInput =
            serde_json::from_value(input).map_err(|e| ToolError::Input(e.to_string()))?;

        let mut req = self.client.get(&args.url);
        if let Some(accept) = args.accept {
            req = req.header(reqwest::header::ACCEPT, accept);
        }
        let resp = req
            .send()
            .await
            .map_err(|e| ToolError::Other(format!("fetch: {e}")))?;

        let status = resp.status();
        let headers = resp.headers().clone();
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| ToolError::Other(format!("body: {e}")))?;
        let truncated = bytes.len() > MAX_BYTES;
        let slice = if truncated {
            &bytes[..MAX_BYTES]
        } else {
            &bytes[..]
        };
        let text = String::from_utf8_lossy(slice).into_owned();

        let ct = headers
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();

        let mut body = text;
        if truncated {
            body.push_str("\n... [truncated]");
        }

        let is_err = !status.is_success();
        let out = if is_err {
            ToolOutput::err(body)
        } else {
            ToolOutput::ok(body)
        };
        Ok(out.with_metadata(json!({
            "status": status.as_u16(),
            "content_type": ct,
            "truncated": truncated,
        })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn ctx() -> ToolContext {
        ToolContext::test(PathBuf::from("/tmp"))
    }

    #[tokio::test]
    async fn happy_path() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/page"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/plain")
                    .set_body_string("hello"),
            )
            .mount(&server)
            .await;

        let t = WebFetchTool::default();
        let url = format!("{}/page", server.uri());
        let out = t.execute(json!({"url": url}), &ctx()).await.unwrap();
        assert!(!out.is_error);
        assert_eq!(out.content, "hello");
    }

    #[tokio::test]
    async fn non_success_marks_error() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/page"))
            .respond_with(ResponseTemplate::new(404).set_body_string("missing"))
            .mount(&server)
            .await;

        let t = WebFetchTool::default();
        let out = t
            .execute(json!({"url": format!("{}/page", server.uri())}), &ctx())
            .await
            .unwrap();
        assert!(out.is_error);
        assert!(out.content.contains("missing"));
    }

    #[tokio::test]
    async fn respects_accept_header() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/page"))
            .and(wiremock::matchers::header("accept", "application/json"))
            .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
            .mount(&server)
            .await;

        let t = WebFetchTool::default();
        let out = t
            .execute(
                json!({
                    "url": format!("{}/page", server.uri()),
                    "accept": "application/json"
                }),
                &ctx(),
            )
            .await
            .unwrap();
        assert!(!out.is_error);
    }

    #[tokio::test]
    async fn truncates_large_bodies() {
        let big = "x".repeat(MAX_BYTES + 1024);
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/big"))
            .respond_with(ResponseTemplate::new(200).set_body_string(big))
            .mount(&server)
            .await;

        let t = WebFetchTool::default();
        let out = t
            .execute(json!({"url": format!("{}/big", server.uri())}), &ctx())
            .await
            .unwrap();
        assert!(out.content.ends_with("[truncated]"));
        let meta = out.metadata.unwrap();
        assert_eq!(meta["truncated"], true);
    }
}
