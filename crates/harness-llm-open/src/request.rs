use harness_llm_types::{ChatOptions, ChatRequest, ContentBlock, Message, MessageRole};
use serde_json::{Map, Value, json};

#[must_use]
pub fn build_chat_body(model: &str, req: &ChatRequest, opts: &ChatOptions) -> Value {
    let mut body = Map::new();
    body.insert("model".into(), Value::String(model.into()));
    body.insert("stream".into(), Value::Bool(true));

    let mut messages = Vec::with_capacity(req.messages.len() + 1);
    if let Some(sys) = &req.system {
        messages.push(json!({"role": "system", "content": sys}));
    }
    for m in &req.messages {
        messages.push(message_to_ollama(m));
    }
    body.insert("messages".into(), Value::Array(messages));

    if !req.tools.is_empty() {
        body.insert(
            "tools".into(),
            Value::Array(
                req.tools
                    .iter()
                    .map(|t| {
                        json!({
                            "type": "function",
                            "function": {
                                "name": t.name,
                                "description": t.description,
                                "parameters": t.input_schema,
                            }
                        })
                    })
                    .collect(),
            ),
        );
    }

    let mut options = Map::new();
    if let Some(t) = opts.temperature {
        options.insert("temperature".into(), json!(t));
    }
    if let Some(mt) = opts.max_tokens {
        options.insert("num_predict".into(), json!(mt));
    }
    if let Some(p) = opts.top_p {
        options.insert("top_p".into(), json!(p));
    }
    if let Some(stops) = &opts.stop_sequences {
        options.insert("stop".into(), json!(stops));
    }
    if !options.is_empty() {
        body.insert("options".into(), Value::Object(options));
    }

    if let Some(o) = &opts.provider.ollama {
        for (k, v) in &o.extra {
            body.insert(k.clone(), v.clone());
        }
    }

    Value::Object(body)
}

fn role(r: MessageRole) -> &'static str {
    match r {
        MessageRole::User => "user",
        MessageRole::Assistant => "assistant",
        MessageRole::System => "system",
    }
}

fn message_to_ollama(m: &Message) -> Value {
    let mut text = String::new();
    let mut images: Vec<String> = Vec::new();
    let mut tool_calls = Vec::new();
    for b in &m.content {
        match b {
            ContentBlock::Text { text: t } => {
                text.push_str(t);
            }
            ContentBlock::Image { data, .. } => {
                images.push(base64_encode(data));
            }
            ContentBlock::ToolCall { name, input, .. } => {
                tool_calls.push(json!({
                    "function": {
                        "name": name,
                        "arguments": input,
                    }
                }));
            }
            ContentBlock::ToolResult { output, .. } => {
                text.push_str(output);
            }
        }
    }
    let mut obj = Map::new();
    obj.insert("role".into(), Value::String(role(m.role).into()));
    obj.insert("content".into(), Value::String(text));
    if !images.is_empty() {
        obj.insert(
            "images".into(),
            Value::Array(images.into_iter().map(Value::String).collect()),
        );
    }
    if !tool_calls.is_empty() {
        obj.insert("tool_calls".into(), Value::Array(tool_calls));
    }
    Value::Object(obj)
}

fn base64_encode(data: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    let mut i = 0;
    while i + 3 <= data.len() {
        let n = (u32::from(data[i]) << 16) | (u32::from(data[i + 1]) << 8) | u32::from(data[i + 2]);
        for shift in [18u32, 12, 6, 0] {
            out.push(CHARS[((n >> shift) & 0x3F) as usize] as char);
        }
        i += 3;
    }
    let rem = data.len() - i;
    if rem == 1 {
        let n = u32::from(data[i]) << 16;
        out.push(CHARS[((n >> 18) & 0x3F) as usize] as char);
        out.push(CHARS[((n >> 12) & 0x3F) as usize] as char);
        out.push_str("==");
    } else if rem == 2 {
        let n = (u32::from(data[i]) << 16) | (u32::from(data[i + 1]) << 8);
        out.push(CHARS[((n >> 18) & 0x3F) as usize] as char);
        out.push(CHARS[((n >> 12) & 0x3F) as usize] as char);
        out.push(CHARS[((n >> 6) & 0x3F) as usize] as char);
        out.push('=');
    }
    out
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
        let body = build_chat_body("llama3:8b", &req, &ChatOptions::default());
        assert_eq!(body["model"], "llama3:8b");
        assert_eq!(body["stream"], true);
        assert_eq!(body["messages"][0]["role"], "user");
        assert_eq!(body["messages"][0]["content"], "hi");
    }

    #[test]
    fn system_prompt_emits_leading_system_message() {
        let req = ChatRequest {
            system: Some("sys".into()),
            messages: vec![Message::user_text("x")],
            tools: vec![],
        };
        let body = build_chat_body("m", &req, &ChatOptions::default());
        assert_eq!(body["messages"][0]["role"], "system");
    }

    #[test]
    fn max_tokens_maps_to_num_predict() {
        let opts = ChatOptions {
            max_tokens: Some(128),
            ..Default::default()
        };
        let req = ChatRequest {
            system: None,
            messages: vec![Message::user_text("x")],
            tools: vec![],
        };
        let body = build_chat_body("m", &req, &opts);
        assert_eq!(body["options"]["num_predict"], 128);
    }
}
