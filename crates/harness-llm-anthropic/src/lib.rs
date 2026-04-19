mod request;
mod sse;

use async_trait::async_trait;
use harness_llm::{BoxStream, Provider};
use harness_llm_types::{ChatOptions, ChatRequest, ProviderError};
use reqwest::{Client, header};
use std::time::Duration;
use tracing::debug;

const API_BASE: &str = "https://api.anthropic.com";
const API_VERSION: &str = "2023-06-01";

pub struct AnthropicProvider {
    id: String,
    api_key: String,
    base_url: String,
    client: Client,
}

impl AnthropicProvider {
    pub fn new(id: impl Into<String>, api_key: impl Into<String>) -> Self {
        Self::with_base_url(id, api_key, API_BASE)
    }

    pub fn with_base_url(
        id: impl Into<String>,
        api_key: impl Into<String>,
        base_url: impl Into<String>,
    ) -> Self {
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .build()
            .expect("reqwest client");
        Self {
            id: id.into(),
            api_key: api_key.into(),
            base_url: base_url.into(),
            client,
        }
    }

    async fn raw_post(
        &self,
        path: &str,
        body: serde_json::Value,
    ) -> Result<reqwest::Response, ProviderError> {
        let url = format!("{}{}", self.base_url, path);
        debug!(url = %url, "anthropic POST");
        self.client
            .post(url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", API_VERSION)
            .header(header::CONTENT_TYPE, "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| ProviderError::Network(e.to_string()))
    }
}

#[async_trait]
impl Provider for AnthropicProvider {
    fn id(&self) -> &str {
        &self.id
    }

    fn family(&self) -> &'static str {
        "anthropic"
    }

    async fn list_models(&self) -> Result<Vec<String>, ProviderError> {
        Ok(vec![
            "claude-opus-4-6".into(),
            "claude-sonnet-4-6".into(),
            "claude-haiku-4-5".into(),
        ])
    }

    async fn chat(
        &self,
        model: &str,
        request: ChatRequest,
        options: ChatOptions,
    ) -> Result<BoxStream, ProviderError> {
        let body = request::build_messages_body(model, &request, &options);
        let resp = self.raw_post("/v1/messages", body).await?;

        let status = resp.status();
        if !status.is_success() {
            let bytes = resp
                .bytes()
                .await
                .map_err(|e| ProviderError::Network(e.to_string()))?;
            let text = String::from_utf8_lossy(&bytes);
            return Err(map_http_error(status, text.into_owned()));
        }

        let byte_stream = resp.bytes_stream();
        let events = sse::parse_sse(byte_stream);
        Ok(Box::pin(events))
    }
}

fn map_http_error(status: reqwest::StatusCode, body: String) -> ProviderError {
    match status.as_u16() {
        401 | 403 => ProviderError::AuthError,
        429 => ProviderError::RateLimit { retry_after: None },
        400 if body.contains("context") => ProviderError::ContextTooLong { max: 0, actual: 0 },
        400..=499 => ProviderError::InvalidRequest(body),
        s @ 500..=599 => ProviderError::ServerError {
            status: s,
            message: body,
        },
        other => ProviderError::Network(format!("unexpected status {other}: {body}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;
    use harness_llm_types::{Message, StreamEvent};
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn sample_sse() -> String {
        concat!(
            "event: message_start\n",
            "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"model\":\"claude-sonnet-4-6\",\"usage\":{\"input_tokens\":5,\"output_tokens\":0}}}\n\n",
            "event: content_block_start\n",
            "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
            "event: content_block_delta\n",
            "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"hello\"}}\n\n",
            "event: content_block_stop\n",
            "data: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
            "event: message_delta\n",
            "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":1}}\n\n",
            "event: message_stop\n",
            "data: {\"type\":\"message_stop\"}\n\n"
        )
        .into()
    }

    #[tokio::test]
    async fn happy_path_streams_delta_and_done() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .and(header("x-api-key", "sk-test"))
            .and(header("anthropic-version", API_VERSION))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(sample_sse()),
            )
            .mount(&server)
            .await;

        let p = AnthropicProvider::with_base_url("test", "sk-test", server.uri());
        let req = ChatRequest {
            system: None,
            messages: vec![Message::user_text("hi")],
            tools: vec![],
        };
        let mut stream = p
            .chat("claude-sonnet-4-6", req, ChatOptions::default())
            .await
            .unwrap();

        let mut got_delta = false;
        let mut got_done = false;
        while let Some(ev) = stream.next().await {
            match ev.unwrap() {
                StreamEvent::Delta { content } => {
                    assert_eq!(content, "hello");
                    got_delta = true;
                }
                StreamEvent::Done { .. } => {
                    got_done = true;
                }
                _ => {}
            }
        }
        assert!(got_delta, "expected at least one delta");
        assert!(got_done, "expected Done");
    }

    #[tokio::test]
    async fn http_401_maps_to_auth_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(ResponseTemplate::new(401).set_body_string("unauthorized"))
            .mount(&server)
            .await;

        let p = AnthropicProvider::with_base_url("test", "bad", server.uri());
        let result = p
            .chat(
                "claude-sonnet-4-6",
                ChatRequest {
                    system: None,
                    messages: vec![Message::user_text("x")],
                    tools: vec![],
                },
                ChatOptions::default(),
            )
            .await;
        let Err(err) = result else {
            panic!("expected error");
        };
        assert!(matches!(err, ProviderError::AuthError));
    }

    #[tokio::test]
    async fn http_429_maps_to_rate_limit() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(ResponseTemplate::new(429).set_body_string("slow down"))
            .mount(&server)
            .await;

        let p = AnthropicProvider::with_base_url("test", "x", server.uri());
        let result = p
            .chat(
                "claude-sonnet-4-6",
                ChatRequest {
                    system: None,
                    messages: vec![Message::user_text("x")],
                    tools: vec![],
                },
                ChatOptions::default(),
            )
            .await;
        let Err(err) = result else {
            panic!("expected error");
        };
        assert!(matches!(err, ProviderError::RateLimit { .. }));
    }

    #[tokio::test]
    async fn http_500_maps_to_server_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(ResponseTemplate::new(502).set_body_string("bad gateway"))
            .mount(&server)
            .await;

        let p = AnthropicProvider::with_base_url("test", "x", server.uri());
        let result = p
            .chat(
                "claude-sonnet-4-6",
                ChatRequest {
                    system: None,
                    messages: vec![Message::user_text("x")],
                    tools: vec![],
                },
                ChatOptions::default(),
            )
            .await;
        let Err(err) = result else {
            panic!("expected error");
        };
        assert!(matches!(
            err,
            ProviderError::ServerError { status: 502, .. }
        ));
    }

    #[test]
    fn family_id_is_anthropic() {
        let p = AnthropicProvider::new("test", "x");
        assert_eq!(p.family(), "anthropic");
        assert_eq!(p.id(), "test");
    }
}
