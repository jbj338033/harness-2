use harness_llm_types::{
    AnthropicOptions, ChatOptions, ChatRequest, ContentBlock, Message, MessageRole,
};
use serde_json::{Map, Value, json};

#[must_use]
pub fn build_messages_body(model: &str, req: &ChatRequest, opts: &ChatOptions) -> Value {
    let mut body = Map::new();
    body.insert("model".into(), Value::String(model.into()));
    body.insert(
        "max_tokens".into(),
        Value::from(opts.max_tokens.unwrap_or(4096)),
    );
    body.insert("stream".into(), Value::Bool(true));

    if let Some(sys) = &req.system {
        body.insert("system".into(), Value::String(sys.clone()));
    }
    if let Some(t) = opts.temperature {
        body.insert("temperature".into(), json!(t));
    }
    if let Some(p) = opts.top_p {
        body.insert("top_p".into(), json!(p));
    }
    if let Some(stops) = &opts.stop_sequences {
        body.insert("stop_sequences".into(), json!(stops));
    }

    body.insert(
        "messages".into(),
        Value::Array(req.messages.iter().map(message_to_anthropic).collect()),
    );

    if !req.tools.is_empty() {
        body.insert(
            "tools".into(),
            Value::Array(
                req.tools
                    .iter()
                    .map(|t| {
                        json!({
                            "name": t.name,
                            "description": t.description,
                            "input_schema": t.input_schema,
                        })
                    })
                    .collect(),
            ),
        );
    }

    if let Some(a) = &opts.provider.anthropic {
        merge_anthropic_options(&mut body, a);
    }

    Value::Object(body)
}

fn role(r: MessageRole) -> &'static str {
    match r {
        MessageRole::User | MessageRole::System => "user",
        MessageRole::Assistant => "assistant",
    }
}

fn message_to_anthropic(m: &Message) -> Value {
    let content: Vec<Value> = m.content.iter().map(block_to_anthropic).collect();
    json!({
        "role": role(m.role),
        "content": content,
    })
}

fn block_to_anthropic(b: &ContentBlock) -> Value {
    match b {
        ContentBlock::Text { text } => json!({ "type": "text", "text": text }),
        ContentBlock::Image { data, media_type } => {
            let encoded = {
                const CHARS: &[u8] =
                    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
                let mut out = String::with_capacity(data.len() * 4 / 3 + 4);
                let mut i = 0;
                while i + 3 <= data.len() {
                    let n = (u32::from(data[i]) << 16)
                        | (u32::from(data[i + 1]) << 8)
                        | u32::from(data[i + 2]);
                    for shift in [18u32, 12, 6, 0] {
                        let idx = ((n >> shift) & 0x3F) as usize;
                        out.push(CHARS[idx] as char);
                    }
                    i += 3;
                }
                let remaining = data.len() - i;
                if remaining == 1 {
                    let n = u32::from(data[i]) << 16;
                    out.push(CHARS[((n >> 18) & 0x3F) as usize] as char);
                    out.push(CHARS[((n >> 12) & 0x3F) as usize] as char);
                    out.push_str("==");
                } else if remaining == 2 {
                    let n = (u32::from(data[i]) << 16) | (u32::from(data[i + 1]) << 8);
                    out.push(CHARS[((n >> 18) & 0x3F) as usize] as char);
                    out.push(CHARS[((n >> 12) & 0x3F) as usize] as char);
                    out.push(CHARS[((n >> 6) & 0x3F) as usize] as char);
                    out.push('=');
                }
                out
            };
            json!({
                "type": "image",
                "source": {
                    "type": "base64",
                    "media_type": media_type,
                    "data": encoded,
                }
            })
        }
        ContentBlock::ToolCall { id, name, input } => json!({
            "type": "tool_use",
            "id": id,
            "name": name,
            "input": input,
        }),
        ContentBlock::ToolResult {
            id,
            output,
            is_error,
        } => json!({
            "type": "tool_result",
            "tool_use_id": id,
            "content": output,
            "is_error": is_error,
        }),
    }
}

fn merge_anthropic_options(body: &mut Map<String, Value>, a: &AnthropicOptions) {
    if let Some(t) = &a.thinking {
        let mut obj = Map::new();
        obj.insert(
            "type".into(),
            Value::String(if t.enabled { "enabled" } else { "disabled" }.into()),
        );
        if let Some(b) = t.budget_tokens {
            obj.insert("budget_tokens".into(), Value::from(b));
        }
        body.insert("thinking".into(), Value::Object(obj));
    }

    for (k, v) in &a.extra {
        body.insert(k.clone(), v.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use harness_llm_types::{ChatOptions, Message};

    #[test]
    fn minimal_body_has_required_fields() {
        let req = ChatRequest {
            system: None,
            messages: vec![Message::user_text("hi")],
            tools: vec![],
        };
        let body = build_messages_body("claude-sonnet-4-6", &req, &ChatOptions::default());
        let obj = body.as_object().unwrap();
        assert_eq!(obj["model"], "claude-sonnet-4-6");
        assert_eq!(obj["stream"], true);
        assert!(obj["messages"].is_array());
    }

    #[test]
    fn system_prompt_propagates() {
        let req = ChatRequest {
            system: Some("hello system".into()),
            messages: vec![Message::user_text("x")],
            tools: vec![],
        };
        let body = build_messages_body("m", &req, &ChatOptions::default());
        assert_eq!(body["system"], "hello system");
    }

    #[test]
    fn assistant_role_preserved() {
        let req = ChatRequest {
            system: None,
            messages: vec![Message::assistant_text("hi")],
            tools: vec![],
        };
        let body = build_messages_body("m", &req, &ChatOptions::default());
        assert_eq!(body["messages"][0]["role"], "assistant");
    }

    #[test]
    fn thinking_option_merges() {
        use harness_llm_types::{AnthropicOptions, ProviderOptions, ThinkingConfig};
        let opts = ChatOptions {
            provider: ProviderOptions {
                anthropic: Some(AnthropicOptions {
                    thinking: Some(ThinkingConfig {
                        enabled: true,
                        budget_tokens: Some(5000),
                    }),
                    ..Default::default()
                }),
                ..Default::default()
            },
            ..Default::default()
        };
        let req = ChatRequest {
            system: None,
            messages: vec![Message::user_text("x")],
            tools: vec![],
        };
        let body = build_messages_body("m", &req, &opts);
        assert_eq!(body["thinking"]["type"], "enabled");
        assert_eq!(body["thinking"]["budget_tokens"], 5000);
    }
}
