use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Method {
    #[serde(rename = "ping")]
    Ping,
    #[serde(rename = "status")]
    Status,
    #[serde(rename = "negotiate")]
    Negotiate,

    #[serde(rename = "v1.session.create")]
    SessionCreate,
    #[serde(rename = "v1.session.list")]
    SessionList,
    #[serde(rename = "v1.session.resume")]
    SessionResume,
    #[serde(rename = "v1.session.delete")]
    SessionDelete,

    #[serde(rename = "v1.chat.send")]
    ChatSend,
    #[serde(rename = "v1.chat.cancel")]
    ChatCancel,

    #[serde(rename = "v1.skill.list")]
    SkillList,
    #[serde(rename = "v1.skill.activate")]
    SkillActivate,

    #[serde(rename = "stream.delta")]
    StreamDelta,
    #[serde(rename = "stream.tool_call")]
    StreamToolCall,
    #[serde(rename = "stream.tool_result")]
    StreamToolResult,
    #[serde(rename = "stream.done")]
    StreamDone,
    #[serde(rename = "stream.error")]
    StreamError,
}

impl Method {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Method::Ping => "ping",
            Method::Status => "status",
            Method::Negotiate => "negotiate",
            Method::SessionCreate => "v1.session.create",
            Method::SessionList => "v1.session.list",
            Method::SessionResume => "v1.session.resume",
            Method::SessionDelete => "v1.session.delete",
            Method::ChatSend => "v1.chat.send",
            Method::ChatCancel => "v1.chat.cancel",
            Method::SkillList => "v1.skill.list",
            Method::SkillActivate => "v1.skill.activate",
            Method::StreamDelta => "stream.delta",
            Method::StreamToolCall => "stream.tool_call",
            Method::StreamToolResult => "stream.tool_result",
            Method::StreamDone => "stream.done",
            Method::StreamError => "stream.error",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NegotiateParams {
    pub client_versions: Vec<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NegotiateResult {
    pub selected: u32,
    pub server_versions: Vec<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionCreateParams {
    pub cwd: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub task: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatSendParams {
    pub session_id: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamDeltaParams {
    pub session_id: String,
    pub agent_id: String,
    pub content: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn as_str_matches_serde_rename() {
        for m in [
            Method::Ping,
            Method::Negotiate,
            Method::SessionCreate,
            Method::ChatSend,
            Method::SkillList,
            Method::SkillActivate,
            Method::StreamDelta,
        ] {
            let json = serde_json::to_string(&m).unwrap();
            assert_eq!(json, format!("\"{}\"", m.as_str()));
        }
    }

    #[test]
    fn method_roundtrip() {
        for m in [
            Method::Ping,
            Method::Status,
            Method::Negotiate,
            Method::SessionCreate,
            Method::SessionList,
            Method::SessionResume,
            Method::SessionDelete,
            Method::ChatSend,
            Method::ChatCancel,
            Method::SkillList,
            Method::SkillActivate,
            Method::StreamDelta,
            Method::StreamToolCall,
            Method::StreamToolResult,
            Method::StreamDone,
            Method::StreamError,
        ] {
            let s = serde_json::to_string(&m).unwrap();
            let back: Method = serde_json::from_str(&s).unwrap();
            assert_eq!(m, back);
        }
    }

    #[test]
    fn v1_prefix_on_session_and_chat_methods() {
        assert_eq!(Method::SessionCreate.as_str(), "v1.session.create");
        assert_eq!(Method::ChatSend.as_str(), "v1.chat.send");
        assert_eq!(Method::SkillList.as_str(), "v1.skill.list");
    }

    #[test]
    fn handshake_methods_are_unversioned() {
        assert_eq!(Method::Ping.as_str(), "ping");
        assert_eq!(Method::Status.as_str(), "status");
        assert_eq!(Method::Negotiate.as_str(), "negotiate");
    }

    #[test]
    fn session_create_params_roundtrip() {
        let p = SessionCreateParams {
            cwd: "/tmp/proj".into(),
            task: Some("fix bug".into()),
            model: None,
        };
        let s = serde_json::to_string(&p).unwrap();
        assert!(!s.contains("\"model\""));
        let back: SessionCreateParams = serde_json::from_str(&s).unwrap();
        assert_eq!(back.cwd, "/tmp/proj");
        assert_eq!(back.task.as_deref(), Some("fix bug"));
    }

    #[test]
    fn negotiate_params_roundtrip() {
        let p = NegotiateParams {
            client_versions: vec![1, 2],
        };
        let s = serde_json::to_string(&p).unwrap();
        let back: NegotiateParams = serde_json::from_str(&s).unwrap();
        assert_eq!(back.client_versions, vec![1, 2]);
    }

    #[test]
    fn negotiate_result_roundtrip() {
        let r = NegotiateResult {
            selected: 1,
            server_versions: vec![1],
        };
        let s = serde_json::to_string(&r).unwrap();
        let back: NegotiateResult = serde_json::from_str(&s).unwrap();
        assert_eq!(back.selected, 1);
        assert_eq!(back.server_versions, vec![1]);
    }
}
