mod request;
mod sse;

use async_trait::async_trait;
use harness_auth::oauth::{self, TokenBundle, openai::OAuthError};
use harness_llm::{BoxStream, Provider};
use harness_llm_types::{ChatOptions, ChatRequest, ProviderError};
use harness_storage::{WriterHandle, credentials};
use parking_lot::Mutex;
use reqwest::{Client, header};
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, info};

const API_BASE: &str = "https://api.openai.com";

const CHATGPT_BASE: &str = oauth::CHATGPT_API_BASE;

#[derive(Clone)]
enum AuthMode {
    ApiKey(String),
    Oauth(Arc<OauthHandle>),
}

struct OauthHandle {
    credential_id: String,
    bundle: Mutex<TokenBundle>,
    writer: WriterHandle,
}

pub struct OpenAiProvider {
    id: String,
    auth: AuthMode,
    base_url: String,
    client: Client,
}

impl OpenAiProvider {
    pub fn new(id: impl Into<String>, api_key: impl Into<String>) -> Self {
        Self::with_base_url(id, api_key, API_BASE)
    }

    pub fn with_base_url(
        id: impl Into<String>,
        api_key: impl Into<String>,
        base_url: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            auth: AuthMode::ApiKey(api_key.into()),
            base_url: base_url.into(),
            client: default_client(),
        }
    }

    #[must_use]
    pub fn new_oauth(
        id: impl Into<String>,
        credential_id: impl Into<String>,
        bundle: TokenBundle,
        writer: WriterHandle,
    ) -> Self {
        Self {
            id: id.into(),
            auth: AuthMode::Oauth(Arc::new(OauthHandle {
                credential_id: credential_id.into(),
                bundle: Mutex::new(bundle),
                writer,
            })),
            base_url: CHATGPT_BASE.to_string(),
            client: default_client(),
        }
    }

    async fn bearer(&self) -> Result<String, ProviderError> {
        match &self.auth {
            AuthMode::ApiKey(k) => Ok(k.clone()),
            AuthMode::Oauth(h) => {
                let now = now_unix_s();
                let (needs_refresh, refresh_token) = {
                    let bundle = h.bundle.lock();
                    (bundle.is_stale(now, 60), bundle.refresh_token.clone())
                };
                if !needs_refresh {
                    return Ok(h.bundle.lock().access_token.clone());
                }
                if refresh_token.is_empty() {
                    return Err(ProviderError::AuthError);
                }
                let new_bundle =
                    oauth::refresh_access_token(&refresh_token)
                        .await
                        .map_err(|e| match e {
                            OAuthError::TokenEndpoint {
                                status: 401 | 403, ..
                            } => ProviderError::AuthError,
                            other => ProviderError::Network(other.to_string()),
                        })?;
                let json = serde_json::to_string(&new_bundle)
                    .map_err(|e| ProviderError::Network(e.to_string()))?;
                credentials::replace_value(&h.writer, h.credential_id.clone(), json)
                    .await
                    .map_err(|e| ProviderError::Network(e.to_string()))?;
                info!(credential = %h.credential_id, "openai: refreshed oauth token");
                let access = new_bundle.access_token.clone();
                *h.bundle.lock() = new_bundle;
                Ok(access)
            }
        }
    }

    async fn raw_post(
        &self,
        path: &str,
        body: serde_json::Value,
    ) -> Result<reqwest::Response, ProviderError> {
        let url = format!("{}{}", self.base_url, path);
        let bearer = self.bearer().await?;
        debug!(url = %url, mode = self.auth.mode_label(), "openai POST");
        let mut builder = self
            .client
            .post(url)
            .bearer_auth(&bearer)
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::ACCEPT, "text/event-stream");
        if matches!(self.auth, AuthMode::Oauth(_)) {
            builder = builder.header(
                header::USER_AGENT,
                concat!("harness/", env!("CARGO_PKG_VERSION"), " (codex-compatible)"),
            );
        }
        builder
            .json(&body)
            .send()
            .await
            .map_err(|e| ProviderError::Network(e.to_string()))
    }
}

impl AuthMode {
    fn mode_label(&self) -> &'static str {
        match self {
            AuthMode::ApiKey(_) => "api_key",
            AuthMode::Oauth(_) => "oauth",
        }
    }
}

fn default_client() -> Client {
    Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .build()
        .expect("reqwest client")
}

fn now_unix_s() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
        .unwrap_or_default()
}

#[async_trait]
impl Provider for OpenAiProvider {
    fn id(&self) -> &str {
        &self.id
    }

    fn family(&self) -> &'static str {
        "openai"
    }

    async fn list_models(&self) -> Result<Vec<String>, ProviderError> {
        if matches!(self.auth, AuthMode::Oauth(_)) {
            return Ok(vec![
                "gpt-5.3-codex".into(),
                "gpt-5.2-codex".into(),
                "gpt-5.4".into(),
            ]);
        }
        let url = format!("{}/v1/models", self.base_url);
        let bearer = self.bearer().await?;
        let resp = self
            .client
            .get(url)
            .bearer_auth(&bearer)
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
        let data = json
            .get("data")
            .and_then(serde_json::Value::as_array)
            .cloned()
            .unwrap_or_default();
        Ok(data
            .into_iter()
            .filter_map(|m| {
                m.get("id")
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
        let body = request::build_completions_body(model, &request, &options);
        let path = match &self.auth {
            AuthMode::ApiKey(_) => "/v1/chat/completions",
            AuthMode::Oauth(_) => "/codex/responses",
        };
        let resp = self.raw_post(path, body).await?;

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
        400 if body.contains("context") || body.contains("maximum context") => {
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
            "data: {\"id\":\"c1\",\"object\":\"chat.completion.chunk\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\"}}]}\n\n",
            "data: {\"id\":\"c1\",\"object\":\"chat.completion.chunk\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"hello\"}}]}\n\n",
            "data: {\"id\":\"c1\",\"object\":\"chat.completion.chunk\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":5,\"completion_tokens\":1,\"total_tokens\":6}}\n\n",
            "data: [DONE]\n\n"
        )
        .into()
    }

    #[tokio::test]
    async fn happy_path_streams_delta_and_done() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .and(header("authorization", "Bearer sk-test"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(sample_sse()),
            )
            .mount(&server)
            .await;

        let p = OpenAiProvider::with_base_url("test", "sk-test", server.uri());
        let req = ChatRequest {
            system: None,
            messages: vec![Message::user_text("hi")],
            tools: vec![],
        };
        let mut stream = p
            .chat("gpt-5.4", req, ChatOptions::default())
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
    async fn http_401_maps_to_auth_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(401).set_body_string("unauthorized"))
            .mount(&server)
            .await;

        let p = OpenAiProvider::with_base_url("test", "bad", server.uri());
        let result = p
            .chat(
                "gpt-5.4",
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

    #[tokio::test]
    async fn http_429_maps_to_rate_limit() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(429).set_body_string("slow down"))
            .mount(&server)
            .await;

        let p = OpenAiProvider::with_base_url("test", "x", server.uri());
        let result = p
            .chat(
                "gpt-5.4",
                ChatRequest {
                    system: None,
                    messages: vec![Message::user_text("x")],
                    tools: vec![],
                },
                ChatOptions::default(),
            )
            .await;
        assert!(matches!(result, Err(ProviderError::RateLimit { .. })));
    }

    #[tokio::test]
    async fn list_models_parses_data_field() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/models"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "object": "list",
                "data": [
                    {"id": "gpt-5.4"},
                    {"id": "o3"},
                    {"id": "o4-mini"}
                ]
            })))
            .mount(&server)
            .await;

        let p = OpenAiProvider::with_base_url("test", "sk", server.uri());
        let models = p.list_models().await.unwrap();
        assert_eq!(models, vec!["gpt-5.4", "o3", "o4-mini"]);
    }

    #[test]
    fn family_id_is_openai() {
        let p = OpenAiProvider::new("test", "x");
        assert_eq!(p.family(), "openai");
        assert_eq!(p.id(), "test");
    }
}
