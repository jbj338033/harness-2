use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const JSONRPC_VERSION: &str = "2.0";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Id {
    Number(i64),
    String(String),
    Null,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    pub jsonrpc: String,
    pub id: Id,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub params: Option<Value>,
}

impl Request {
    #[must_use]
    pub fn new(id: Id, method: impl Into<String>, params: Option<Value>) -> Self {
        Self {
            jsonrpc: JSONRPC_VERSION.into(),
            id,
            method: method.into(),
            params,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notification {
    pub jsonrpc: String,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub params: Option<Value>,
}

impl Notification {
    #[must_use]
    pub fn new(method: impl Into<String>, params: Option<Value>) -> Self {
        Self {
            jsonrpc: JSONRPC_VERSION.into(),
            method: method.into(),
            params,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    pub jsonrpc: String,
    pub id: Id,
    #[serde(flatten)]
    pub payload: ResponsePayload,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum ResponsePayload {
    #[serde(rename = "result")]
    Result(Value),
    #[serde(rename = "error")]
    Error(ErrorObject),
}

impl Response {
    #[must_use]
    pub fn ok(id: Id, result: Value) -> Self {
        Self {
            jsonrpc: JSONRPC_VERSION.into(),
            id,
            payload: ResponsePayload::Result(result),
        }
    }

    #[must_use]
    pub fn err(id: Id, error: ErrorObject) -> Self {
        Self {
            jsonrpc: JSONRPC_VERSION.into(),
            id,
            payload: ResponsePayload::Error(error),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ErrorObject {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub data: Option<Value>,
}

impl ErrorObject {
    #[must_use]
    pub fn new(code: impl Into<i32>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            data: None,
        }
    }

    #[must_use]
    pub fn with_data(mut self, data: Value) -> Self {
        self.data = Some(data);
        self
    }
}

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    ParseError = -32_700,
    InvalidRequest = -32_600,
    MethodNotFound = -32_601,
    InvalidParams = -32_602,
    InternalError = -32_603,

    Unauthorized = -32_001,
    Forbidden = -32_002,
    NotFound = -32_004,
    VersionMismatch = -32_010,
    RateLimited = -32_029,
}

impl From<ErrorCode> for i32 {
    fn from(c: ErrorCode) -> Self {
        c as i32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn request_serializes_with_version() {
        let req = Request::new(Id::Number(1), "chat.send", Some(json!({"text": "hi"})));
        let s = serde_json::to_string(&req).unwrap();
        assert!(s.contains("\"jsonrpc\":\"2.0\""));
        assert!(s.contains("\"method\":\"chat.send\""));
    }

    #[test]
    fn request_roundtrip() {
        let req = Request::new(Id::String("abc".into()), "session.create", None);
        let s = serde_json::to_string(&req).unwrap();
        let back: Request = serde_json::from_str(&s).unwrap();
        assert_eq!(back.method, "session.create");
        assert_eq!(back.id, Id::String("abc".into()));
        assert!(back.params.is_none());
    }

    #[test]
    fn response_ok_serializes_result_field() {
        let r = Response::ok(Id::Number(2), json!({"ok": true}));
        let s = serde_json::to_string(&r).unwrap();
        assert!(s.contains("\"result\":"));
        assert!(!s.contains("\"error\":"));
    }

    #[test]
    fn response_err_serializes_error_field() {
        let r = Response::err(
            Id::Number(3),
            ErrorObject::new(ErrorCode::MethodNotFound, "no such method"),
        );
        let s = serde_json::to_string(&r).unwrap();
        assert!(s.contains("\"error\":"));
        assert!(!s.contains("\"result\":"));
        assert!(s.contains("-32601"));
    }

    #[test]
    fn notification_has_no_id_field() {
        let n = Notification::new("stream.delta", Some(json!({"content": "hi"})));
        let s = serde_json::to_string(&n).unwrap();
        assert!(!s.contains("\"id\""));
    }

    #[test]
    fn id_accepts_number_string_and_null() {
        let n: Id = serde_json::from_str("1").unwrap();
        assert_eq!(n, Id::Number(1));
        let s: Id = serde_json::from_str("\"abc\"").unwrap();
        assert_eq!(s, Id::String("abc".into()));
        let z: Id = serde_json::from_str("null").unwrap();
        assert_eq!(z, Id::Null);
    }

    #[test]
    fn error_codes_have_stable_values() {
        assert_eq!(i32::from(ErrorCode::ParseError), -32_700);
        assert_eq!(i32::from(ErrorCode::MethodNotFound), -32_601);
        assert_eq!(i32::from(ErrorCode::Unauthorized), -32_001);
    }
}
