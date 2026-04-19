mod ndjson;
mod request;

use async_trait::async_trait;
use harness_llm::{BoxStream, Provider};
use harness_llm_types::{ChatOptions, ChatRequest, ProviderError};
use reqwest::{Client, header};
use std::time::Duration;
use tracing::debug;

const API_BASE: &str = "http://localhost:11434";

pub struct OllamaProvider {
    id: String,
    base_url: String,
    client: Client,
}

impl OllamaProvider {
    pub fn new(id: impl Into<String>) -> Self {
        Self::with_base_url(id, API_BASE)
    }

    pub fn with_base_url(id: impl Into<String>, base_url: impl Into<String>) -> Self {
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .build()
            .expect("reqwest client");
        Self {
            id: id.into(),
            base_url: base_url.into(),
            client,
        }
    }
}

#[async_trait]
impl Provider for OllamaProvider {
    fn id(&self) -> &str {
        &self.id
    }

    fn family(&self) -> &'static str {
        "ollama"
    }

    async fn list_models(&self) -> Result<Vec<String>, ProviderError> {
        let url = format!("{}/api/tags", self.base_url);
        let resp = self
            .client
            .get(url)
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
        Ok(json
            .get("models")
            .and_then(serde_json::Value::as_array)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|m| {
                m.get("name")
                    .and_then(serde_json::Value::as_str)
                    .map(String::from)
            })
            .collect())
    }

    async fn chat(
        &self,
        model: &str,
        request: ChatRequest,
        options: ChatOptions,
    ) -> Result<BoxStream, ProviderError> {
        let body = request::build_chat_body(model, &request, &options);
        let url = format!("{}/api/chat", self.base_url);
        debug!(url = %url, "ollama POST");
        let resp = self
            .client
            .post(url)
            .header(header::CONTENT_TYPE, "application/json")
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
        Ok(Box::pin(ndjson::parse_ndjson(byte_stream)))
    }
}

fn map_http_error(status: reqwest::StatusCode, body: String) -> ProviderError {
    match status.as_u16() {
        401 | 403 => ProviderError::AuthError,
        429 => ProviderError::RateLimit { retry_after: None },
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
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn sample_ndjson() -> String {
        concat!(
            "{\"model\":\"llama3:8b\",\"message\":{\"role\":\"assistant\",\"content\":\"hi\"},\"done\":false}\n",
            "{\"model\":\"llama3:8b\",\"message\":{\"role\":\"assistant\",\"content\":\" there\"},\"done\":false}\n",
            "{\"model\":\"llama3:8b\",\"done\":true,\"prompt_eval_count\":5,\"eval_count\":2}\n"
        )
        .into()
    }

    #[tokio::test]
    async fn happy_path_streams_and_completes() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/chat"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "application/x-ndjson")
                    .set_body_string(sample_ndjson()),
            )
            .mount(&server)
            .await;

        let p = OllamaProvider::with_base_url("test", server.uri());
        let mut stream = p
            .chat(
                "llama3:8b",
                ChatRequest {
                    system: None,
                    messages: vec![Message::user_text("hi")],
                    tools: vec![],
                },
                ChatOptions::default(),
            )
            .await
            .unwrap();

        let mut text = String::new();
        let mut got_done = false;
        while let Some(ev) = stream.next().await {
            match ev.unwrap() {
                StreamEvent::Delta { content } => text.push_str(&content),
                StreamEvent::Done { usage } => {
                    assert_eq!(usage.prompt_tokens, 5);
                    assert_eq!(usage.completion_tokens, 2);
                    got_done = true;
                }
                _ => {}
            }
        }
        assert_eq!(text, "hi there");
        assert!(got_done);
    }

    #[tokio::test]
    async fn list_models_parses_tags() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/tags"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "models": [
                    {"name": "llama3:8b"},
                    {"name": "codestral:22b"}
                ]
            })))
            .mount(&server)
            .await;

        let p = OllamaProvider::with_base_url("t", server.uri());
        let models = p.list_models().await.unwrap();
        assert_eq!(models, vec!["llama3:8b", "codestral:22b"]);
    }

    #[test]
    fn family_id_is_ollama() {
        let p = OllamaProvider::new("test");
        assert_eq!(p.family(), "ollama");
    }
}
