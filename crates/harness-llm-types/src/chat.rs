use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    User,
    Assistant,
    System,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text {
        text: String,
    },
    Image {
        data: Vec<u8>,
        media_type: String,
    },
    ToolCall {
        id: String,
        name: String,
        input: Value,
    },
    ToolResult {
        id: String,
        output: String,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        is_error: bool,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: MessageRole,
    pub content: Vec<ContentBlock>,
}

impl Message {
    #[must_use]
    pub fn user_text(text: impl Into<String>) -> Self {
        Self {
            role: MessageRole::User,
            content: vec![ContentBlock::Text { text: text.into() }],
        }
    }

    #[must_use]
    pub fn assistant_text(text: impl Into<String>) -> Self {
        Self {
            role: MessageRole::Assistant,
            content: vec![ContentBlock::Text { text: text.into() }],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn text_message_roundtrip() {
        let m = Message::user_text("hello");
        let s = serde_json::to_string(&m).unwrap();
        assert!(s.contains("\"type\":\"text\""));
        let back: Message = serde_json::from_str(&s).unwrap();
        assert_eq!(back.role, MessageRole::User);
    }

    #[test]
    fn tool_call_roundtrip() {
        let m = Message {
            role: MessageRole::Assistant,
            content: vec![ContentBlock::ToolCall {
                id: "t1".into(),
                name: "bash".into(),
                input: json!({"command": "ls"}),
            }],
        };
        let s = serde_json::to_string(&m).unwrap();
        let back: Message = serde_json::from_str(&s).unwrap();
        match &back.content[0] {
            ContentBlock::ToolCall { name, .. } => assert_eq!(name, "bash"),
            _ => panic!("expected tool call"),
        }
    }

    #[test]
    fn tool_def_schema_preserved() {
        let t = ToolDef {
            name: "read".into(),
            description: "Read a file".into(),
            input_schema: json!({"type": "object"}),
        };
        let s = serde_json::to_string(&t).unwrap();
        let back: ToolDef = serde_json::from_str(&s).unwrap();
        assert_eq!(back.name, "read");
    }
}
