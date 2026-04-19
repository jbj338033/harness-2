use harness_llm_types::{
    ChatOptions, ChatRequest, ContentBlock, Message, MessageRole, OpenAiOptions, ReasoningEffort,
    ResponseFormat,
};
use serde_json::{Map, Value, json};

#[must_use]
pub fn build_completions_body(model: &str, req: &ChatRequest, opts: &ChatOptions) -> Value {
    let mut body = Map::new();
    body.insert("model".into(), Value::String(model.into()));
    body.insert("stream".into(), Value::Bool(true));
    body.insert("stream_options".into(), json!({"include_usage": true}));

    let mut messages = Vec::with_capacity(req.messages.len() + 1);
    if let Some(sys) = &req.system {
        messages.push(json!({"role": "system", "content": sys}));
    }
    for m in &req.messages {
        messages.push(message_to_openai(m));
    }
    body.insert("messages".into(), Value::Array(messages));

    if let Some(t) = opts.temperature {
        body.insert("temperature".into(), json!(t));
    }
    if let Some(mt) = opts.max_tokens {
        body.insert("max_completion_tokens".into(), json!(mt));
    }
    if let Some(p) = opts.top_p {
        body.insert("top_p".into(), json!(p));
    }
    if let Some(stops) = &opts.stop_sequences {
        body.insert("stop".into(), json!(stops));
    }

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
                                "parameters": normalize_parameters(&t.input_schema),
                            }
                        })
                    })
                    .collect(),
            ),
        );
    }

    if let Some(o) = &opts.provider.openai {
        merge_openai_options(&mut body, o);
    }

    Value::Object(body)
}

fn normalize_parameters(schema: &Value) -> Value {
    let Some(obj) = schema.as_object() else {
        return schema.clone();
    };
    let variants = obj
        .get("oneOf")
        .or_else(|| obj.get("anyOf"))
        .and_then(Value::as_array);
    let Some(variants) = variants else {
        return schema.clone();
    };

    let mut props: Map<String, Value> = obj
        .get("properties")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let mut discriminator: Option<String> = None;
    let mut discriminator_values: Vec<String> = Vec::new();

    for variant in variants {
        let Some(vp) = variant.get("properties").and_then(Value::as_object) else {
            continue;
        };
        for (k, v) in vp {
            if let Some(c) = v.get("const").and_then(Value::as_str) {
                discriminator.get_or_insert_with(|| k.clone());
                discriminator_values.push(c.to_string());
            } else {
                props.entry(k.clone()).or_insert_with(|| v.clone());
            }
        }
    }

    if let Some(name) = discriminator {
        discriminator_values.sort();
        discriminator_values.dedup();
        props.insert(
            name,
            json!({"type": "string", "enum": discriminator_values}),
        );
    }

    let mut out = Map::new();
    out.insert("type".into(), Value::String("object".into()));
    out.insert("properties".into(), Value::Object(props));
    if let Some(req) = obj.get("required").cloned() {
        out.insert("required".into(), req);
    }
    for (k, v) in obj {
        if matches!(
            k.as_str(),
            "type" | "properties" | "required" | "oneOf" | "anyOf" | "allOf"
        ) {
            continue;
        }
        out.insert(k.clone(), v.clone());
    }
    Value::Object(out)
}

fn role(r: MessageRole) -> &'static str {
    match r {
        MessageRole::User => "user",
        MessageRole::Assistant => "assistant",
        MessageRole::System => "system",
    }
}

fn message_to_openai(m: &Message) -> Value {
    let mut tool_calls = Vec::new();
    let mut text_parts = Vec::new();
    let mut tool_result: Option<(String, String, bool)> = None;

    for b in &m.content {
        match b {
            ContentBlock::Text { text } => text_parts.push(json!({"type": "text", "text": text})),
            ContentBlock::Image { data, media_type } => {
                let encoded = base64_encode(data);
                let url = format!("data:{media_type};base64,{encoded}");
                text_parts.push(json!({
                    "type": "image_url",
                    "image_url": {"url": url}
                }));
            }
            ContentBlock::ToolCall { id, name, input } => {
                tool_calls.push(json!({
                    "id": id,
                    "type": "function",
                    "function": {
                        "name": name,
                        "arguments": input.to_string(),
                    }
                }));
            }
            ContentBlock::ToolResult {
                id,
                output,
                is_error,
            } => {
                tool_result = Some((id.clone(), output.clone(), *is_error));
            }
        }
    }

    if let Some((id, output, _is_error)) = tool_result {
        return json!({
            "role": "tool",
            "tool_call_id": id,
            "content": output,
        });
    }

    let mut obj = Map::new();
    obj.insert("role".into(), Value::String(role(m.role).into()));
    if !text_parts.is_empty() {
        obj.insert("content".into(), Value::Array(text_parts));
    } else if !tool_calls.is_empty() {
        obj.insert("content".into(), Value::Null);
    } else {
        obj.insert("content".into(), Value::String(String::new()));
    }
    if !tool_calls.is_empty() {
        obj.insert("tool_calls".into(), Value::Array(tool_calls));
    }
    Value::Object(obj)
}

fn merge_openai_options(body: &mut Map<String, Value>, o: &OpenAiOptions) {
    if let Some(eff) = o.reasoning_effort {
        let s = match eff {
            ReasoningEffort::Low => "low",
            ReasoningEffort::Medium => "medium",
            ReasoningEffort::High => "high",
        };
        body.insert("reasoning_effort".into(), Value::String(s.into()));
    }
    if let Some(fmt) = &o.response_format {
        let v = match fmt {
            ResponseFormat::Text => json!({"type": "text"}),
            ResponseFormat::JsonObject => json!({"type": "json_object"}),
        };
        body.insert("response_format".into(), v);
    }
    for (k, v) in &o.extra {
        body.insert(k.clone(), v.clone());
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
    use harness_llm_types::{ChatOptions, Message, OpenAiOptions, ProviderOptions};

    #[test]
    fn minimal_body_has_required_fields() {
        let req = ChatRequest {
            system: None,
            messages: vec![Message::user_text("hi")],
            tools: vec![],
        };
        let body = build_completions_body("gpt-5.4", &req, &ChatOptions::default());
        assert_eq!(body["model"], "gpt-5.4");
        assert_eq!(body["stream"], true);
        assert!(body["messages"].is_array());
    }

    #[test]
    fn system_prompt_emits_system_message() {
        let req = ChatRequest {
            system: Some("sys".into()),
            messages: vec![Message::user_text("hi")],
            tools: vec![],
        };
        let body = build_completions_body("m", &req, &ChatOptions::default());
        assert_eq!(body["messages"][0]["role"], "system");
    }

    #[test]
    fn reasoning_effort_propagates() {
        let opts = ChatOptions {
            provider: ProviderOptions {
                openai: Some(OpenAiOptions {
                    reasoning_effort: Some(ReasoningEffort::High),
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
        let body = build_completions_body("o3", &req, &opts);
        assert_eq!(body["reasoning_effort"], "high");
    }

    #[test]
    fn json_response_format_propagates() {
        let opts = ChatOptions {
            provider: ProviderOptions {
                openai: Some(OpenAiOptions {
                    response_format: Some(ResponseFormat::JsonObject),
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
        let body = build_completions_body("m", &req, &opts);
        assert_eq!(body["response_format"]["type"], "json_object");
    }

    #[test]
    fn tool_def_maps_to_openai_function_format() {
        let req = ChatRequest {
            system: None,
            messages: vec![Message::user_text("x")],
            tools: vec![harness_llm_types::ToolDef {
                name: "bash".into(),
                description: "Run a shell command".into(),
                input_schema: serde_json::json!({"type": "object"}),
            }],
        };
        let body = build_completions_body("m", &req, &ChatOptions::default());
        assert_eq!(body["tools"][0]["type"], "function");
        assert_eq!(body["tools"][0]["function"]["name"], "bash");
    }

    #[test]
    fn passthrough_when_no_oneof_at_root() {
        let s = json!({"type": "object", "properties": {"x": {"type": "integer"}}});
        assert_eq!(normalize_parameters(&s), s);
    }

    #[test]
    fn flattens_oneof_with_const_discriminator() {
        let schema = json!({
            "type": "object",
            "required": ["action"],
            "oneOf": [
                {"properties": {"action": {"const": "click"}, "x": {"type": "integer"}}},
                {"properties": {"action": {"const": "type"}, "text": {"type": "string"}}}
            ]
        });
        let out = normalize_parameters(&schema);
        assert_eq!(out["type"], "object");
        assert!(out.get("oneOf").is_none());
        assert_eq!(out["properties"]["action"]["type"], "string");
        let variants: Vec<&str> = out["properties"]["action"]["enum"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert_eq!(variants, vec!["click", "type"]);
        assert_eq!(out["properties"]["x"]["type"], "integer");
        assert_eq!(out["properties"]["text"]["type"], "string");
        assert_eq!(out["required"], json!(["action"]));
    }

    #[test]
    fn tool_parameters_are_normalized_in_body() {
        let req = ChatRequest {
            system: None,
            messages: vec![Message::user_text("x")],
            tools: vec![harness_llm_types::ToolDef {
                name: "t".into(),
                description: "d".into(),
                input_schema: json!({
                    "type": "object",
                    "required": ["action"],
                    "oneOf": [
                        {"properties": {"action": {"const": "a"}}},
                        {"properties": {"action": {"const": "b"}}}
                    ]
                }),
            }],
        };
        let body = build_completions_body("m", &req, &ChatOptions::default());
        let params = &body["tools"][0]["function"]["parameters"];
        assert!(params.get("oneOf").is_none());
        assert_eq!(params["type"], "object");
    }

    #[test]
    fn tool_result_becomes_tool_role() {
        let msg = Message {
            role: MessageRole::User,
            content: vec![ContentBlock::ToolResult {
                id: "t1".into(),
                output: "done".into(),
                is_error: false,
            }],
        };
        let v = message_to_openai(&msg);
        assert_eq!(v["role"], "tool");
        assert_eq!(v["tool_call_id"], "t1");
        assert_eq!(v["content"], "done");
    }
}
