mod request;
mod sse;

use async_trait::async_trait;
use harness_llm::{BoxStream, Provider};
use harness_llm_types::{ChatOptions, ChatRequest, ProviderError};
use reqwest::{Client, header};
use std::time::Duration;
use tracing::debug;

const API_BASE: &str = "https://generativelanguage.googleapis.com";

pub struct GoogleProvider {
    id: String,
    api_key: String,
    base_url: String,
    client: Client,
}

impl GoogleProvider {
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
}

#[async_trait]
impl Provider for GoogleProvider {
    fn id(&self) -> &str {
        &self.id
    }

    fn family(&self) -> &'static str {
        "google"
    }

    async fn list_models(&self) -> Result<Vec<String>, ProviderError> {
        let url = format!("{}/v1beta/models", self.base_url);
        let resp = self
            .client
            .get(url)
            .header("x-goog-api-key", &self.api_key)
            .send()
            .await
            .map_err(|e| ProviderError::Network(e.to_string()))?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(map_http_error(status, body));
        }
        let json: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| ProviderError::Network(e.to_string()))?;
        let items = json
            .get("models")
            .and_then(serde_json::Value::as_array)
            .cloned()
            .unwrap_or_default();
        Ok(items
            .into_iter()
            .filter_map(|m| {
                m.get("name")
                    .and_then(serde_json::Value::as_str)
                    .map(|n| n.trim_start_matches("models/").to_string())
            })
            .collect())
    }

    async fn chat(
        &self,
        model: &str,
        request: ChatRequest,
        options: ChatOptions,
    ) -> Result<BoxStream, ProviderError> {
        let body = request::build_generate_body(&request, &options);
        let url = format!(
            "{}/v1beta/models/{model}:streamGenerateContent?alt=sse",
            self.base_url
        );
        debug!(url = %url, "google POST");
        let resp = self
            .client
            .post(url)
            .header("x-goog-api-key", &self.api_key)
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::ACCEPT, "text/event-stream")
            .json(&body)
            .send()
            .await
            .map_err(|e| ProviderError::Network(e.to_string()))?;

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
        400 if body.contains("exceeds") || body.contains("too large") => {
            ProviderError::ContextTooLong { max: 0, actual: 0 }
        }
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
            "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"hello\"}],\"role\":\"model\"}}]}\n\n",
            "data: {\"candidates\":[{\"content\":{\"parts\":[],\"role\":\"model\"},\"finishReason\":\"STOP\"}],\"usageMetadata\":{\"promptTokenCount\":5,\"candidatesTokenCount\":1}}\n\n"
        )
        .into()
    }

    #[tokio::test]
    async fn happy_path_streams_delta_and_done() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1beta/models/gemini-3.1-pro:streamGenerateContent"))
            .and(header("x-goog-api-key", "ya-test"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(sample_sse()),
            )
            .mount(&server)
            .await;

        let p = GoogleProvider::with_base_url("test", "ya-test", server.uri());
        let req = ChatRequest {
            system: None,
            messages: vec![Message::user_text("hi")],
            tools: vec![],
        };
        let mut stream = p
            .chat("gemini-3.1-pro", req, ChatOptions::default())
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
                StreamEvent::Done { usage } => {
                    assert_eq!(usage.prompt_tokens, 5);
                    assert_eq!(usage.completion_tokens, 1);
                    got_done = true;
                }
                _ => {}
            }
        }
        assert!(got_delta);
        assert!(got_done);
    }

    #[tokio::test]
    async fn list_models_strips_models_prefix() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1beta/models"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "models": [
                    {"name": "models/gemini-3.1-pro"},
                    {"name": "models/gemini-2.5-flash"}
                ]
            })))
            .mount(&server)
            .await;

        let p = GoogleProvider::with_base_url("t", "k", server.uri());
        let models = p.list_models().await.unwrap();
        assert_eq!(models, vec!["gemini-3.1-pro", "gemini-2.5-flash"]);
    }

    #[tokio::test]
    async fn http_401_maps_to_auth_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1beta/models/x:streamGenerateContent"))
            .respond_with(ResponseTemplate::new(403).set_body_string("forbidden"))
            .mount(&server)
            .await;

        let p = GoogleProvider::with_base_url("t", "k", server.uri());
        let result = p
            .chat(
                "x",
                ChatRequest {
                    system: None,
                    messages: vec![Message::user_text("x")],
                    tools: vec![],
                },
                ChatOptions::default(),
            )
            .await;
        assert!(matches!(result, Err(ProviderError::AuthError)));
    }

    #[test]
    fn family_id_is_google() {
        let p = GoogleProvider::new("test", "x");
        assert_eq!(p.family(), "google");
    }
}
