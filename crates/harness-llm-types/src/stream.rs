use crate::chat::{Message, ToolDef};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatRequest {
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub system: Option<String>,
    pub messages: Vec<Message>,
    #[serde(default)]
    pub tools: Vec<ToolDef>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub cache_read_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub cache_creation_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub cost: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StreamEvent {
    Delta { content: String },
    ToolCallStart { id: String, name: String },
    ToolCallDelta { id: String, chunk: String },
    Done { usage: Usage },
    Error { error: ProviderError },
}

#[derive(Debug, Clone, Error, Serialize, Deserialize)]
pub enum ProviderError {
    #[error("rate limited (retry after {retry_after:?})")]
    RateLimit {
        #[serde(default)]
        retry_after: Option<Duration>,
    },
    #[error("context too long: {actual} tokens exceeds {max}")]
    ContextTooLong { max: usize, actual: usize },
    #[error("server error {status}: {message}")]
    ServerError { status: u16, message: String },
    #[error("auth error")]
    AuthError,
    #[error("invalid request: {0}")]
    InvalidRequest(String),
    #[error("stream interrupted")]
    StreamInterrupted,
    #[error("network error: {0}")]
    Network(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::Message;

    #[test]
    fn chat_request_roundtrip() {
        let req = ChatRequest {
            system: Some("You are harness.".into()),
            messages: vec![Message::user_text("hi")],
            tools: vec![],
        };
        let s = serde_json::to_string(&req).unwrap();
        let back: ChatRequest = serde_json::from_str(&s).unwrap();
        assert_eq!(back.system.as_deref(), Some("You are harness."));
        assert_eq!(back.messages.len(), 1);
    }

    #[test]
    fn delta_event_roundtrip() {
        let e = StreamEvent::Delta {
            content: "hello".into(),
        };
        let s = serde_json::to_string(&e).unwrap();
        assert!(s.contains("\"kind\":\"delta\""));
        let back: StreamEvent = serde_json::from_str(&s).unwrap();
        match back {
            StreamEvent::Delta { content } => assert_eq!(content, "hello"),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn rate_limit_serializes_retry_after() {
        let e = ProviderError::RateLimit {
            retry_after: Some(Duration::from_secs(12)),
        };
        let s = serde_json::to_string(&e).unwrap();
        let back: ProviderError = serde_json::from_str(&s).unwrap();
        match back {
            ProviderError::RateLimit {
                retry_after: Some(d),
            } => assert_eq!(d.as_secs(), 12),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn context_too_long_carries_numbers() {
        let e = ProviderError::ContextTooLong {
            max: 200_000,
            actual: 210_000,
        };
        let s = serde_json::to_string(&e).unwrap();
        let back: ProviderError = serde_json::from_str(&s).unwrap();
        match back {
            ProviderError::ContextTooLong { max, actual } => {
                assert_eq!((max, actual), (200_000, 210_000));
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn done_event_carries_usage() {
        let e = StreamEvent::Done {
            usage: Usage {
                prompt_tokens: 100,
                completion_tokens: 50,
                ..Default::default()
            },
        };
        let s = serde_json::to_string(&e).unwrap();
        let back: StreamEvent = serde_json::from_str(&s).unwrap();
        match back {
            StreamEvent::Done { usage } => {
                assert_eq!(usage.prompt_tokens, 100);
                assert_eq!(usage.completion_tokens, 50);
            }
            _ => panic!("wrong variant"),
        }
    }
}
