use harness_llm_types::{ChatOptions, ChatRequest, ContentBlock, Message, MessageRole};
use serde_json::{Map, Value, json};

#[must_use]
pub fn build_generate_body(req: &ChatRequest, opts: &ChatOptions) -> Value {
    let mut body = Map::new();

    if let Some(sys) = &req.system {
        body.insert(
            "systemInstruction".into(),
            json!({"parts": [{"text": sys}]}),
        );
    }

    body.insert(
        "contents".into(),
        Value::Array(req.messages.iter().map(message_to_gemini).collect()),
    );

    if !req.tools.is_empty() {
        body.insert(
            "tools".into(),
            json!([{
                "functionDeclarations": req.tools.iter().map(|t| json!({
                    "name": t.name,
                    "description": t.description,
                    "parameters": t.input_schema,
                })).collect::<Vec<_>>()
            }]),
        );
    }

    let mut gen_config = Map::new();
    if let Some(t) = opts.temperature {
        gen_config.insert("temperature".into(), json!(t));
    }
    if let Some(mt) = opts.max_tokens {
        gen_config.insert("maxOutputTokens".into(), json!(mt));
    }
    if let Some(p) = opts.top_p {
        gen_config.insert("topP".into(), json!(p));
    }
    if let Some(stops) = &opts.stop_sequences {
        gen_config.insert("stopSequences".into(), json!(stops));
    }
    if !gen_config.is_empty() {
        body.insert("generationConfig".into(), Value::Object(gen_config));
    }

    if let Some(g) = &opts.provider.google {
        for (k, v) in &g.extra {
            body.insert(k.clone(), v.clone());
        }
    }

    Value::Object(body)
}

fn role(r: MessageRole) -> &'static str {
    match r {
        MessageRole::Assistant => "model",
        MessageRole::User | MessageRole::System => "user",
    }
}

fn message_to_gemini(m: &Message) -> Value {
    let parts: Vec<Value> = m.content.iter().map(block_to_gemini).collect();
    json!({
        "role": role(m.role),
        "parts": parts,
    })
}

fn block_to_gemini(b: &ContentBlock) -> Value {
    match b {
        ContentBlock::Text { text } => json!({"text": text}),
        ContentBlock::Image { data, media_type } => {
            let encoded = base64_encode(data);
            json!({
                "inlineData": {
                    "mimeType": media_type,
                    "data": encoded,
                }
            })
        }
        ContentBlock::ToolCall { name, input, .. } => json!({
            "functionCall": {
                "name": name,
                "args": input,
            }
        }),
        ContentBlock::ToolResult { id, output, .. } => json!({
            "functionResponse": {
                "name": id,
                "response": {"content": output},
            }
        }),
    }
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
    use harness_llm_types::{ChatOptions, Message, ToolDef};

    #[test]
    fn system_becomes_systeminstruction() {
        let req = ChatRequest {
            system: Some("sys".into()),
            messages: vec![Message::user_text("hi")],
            tools: vec![],
        };
        let body = build_generate_body(&req, &ChatOptions::default());
        assert_eq!(body["systemInstruction"]["parts"][0]["text"], "sys");
    }

    #[test]
    fn assistant_role_maps_to_model() {
        let req = ChatRequest {
            system: None,
            messages: vec![Message::assistant_text("yo")],
            tools: vec![],
        };
        let body = build_generate_body(&req, &ChatOptions::default());
        assert_eq!(body["contents"][0]["role"], "model");
    }

    #[test]
    fn tools_wrap_in_function_declarations() {
        let req = ChatRequest {
            system: None,
            messages: vec![Message::user_text("x")],
            tools: vec![ToolDef {
                name: "bash".into(),
                description: "run".into(),
                input_schema: serde_json::json!({"type": "object"}),
            }],
        };
        let body = build_generate_body(&req, &ChatOptions::default());
        assert_eq!(body["tools"][0]["functionDeclarations"][0]["name"], "bash");
    }

    #[test]
    fn max_tokens_becomes_max_output_tokens() {
        let opts = ChatOptions {
            max_tokens: Some(256),
            ..Default::default()
        };
        let req = ChatRequest {
            system: None,
            messages: vec![Message::user_text("x")],
            tools: vec![],
        };
        let body = build_generate_body(&req, &opts);
        assert_eq!(body["generationConfig"]["maxOutputTokens"], 256);
    }
}
