use bytes::Bytes;
use futures::stream::{Stream, StreamExt};
use harness_llm_types::{ProviderError, StreamEvent, Usage};
use serde_json::Value;
use tracing::{debug, warn};

pub fn parse_sse<S>(stream: S) -> impl Stream<Item = Result<StreamEvent, ProviderError>> + Send
where
    S: Stream<Item = Result<Bytes, reqwest::Error>> + Send + 'static,
{
    async_stream::stream! {
        futures::pin_mut!(stream);
        let mut buffer = String::new();
        let mut usage = Usage::default();
        let mut tool_counter: u64 = 0;
        let mut final_emitted = false;

        while let Some(chunk) = stream.next().await {
            let bytes = match chunk {
                Ok(b) => b,
                Err(e) => {
                    warn!(error = %e, "gemini byte stream error");
                    yield Err(ProviderError::StreamInterrupted);
                    return;
                }
            };
            buffer.push_str(&String::from_utf8_lossy(&bytes));

            while let Some(pos) = buffer.find("\n\n") {
                let frame = buffer[..pos].to_string();
                buffer.drain(..pos + 2);

                for line in frame.lines() {
                    let line = line.trim_start();
                    let data = match line.strip_prefix("data:") {
                        Some(d) => d.trim(),
                        None => continue,
                    };
                    if data.is_empty() {
                        continue;
                    }
                    let json: Value = match serde_json::from_str(data) {
                        Ok(v) => v,
                        Err(e) => {
                            debug!(error = %e, data = %data, "gemini json parse failed");
                            continue;
                        }
                    };

                    if let Some(u) = json.get("usageMetadata") {
                        ingest_usage(&mut usage, u);
                    }

                    let Some(candidates) = json.get("candidates").and_then(Value::as_array) else {
                        continue;
                    };
                    for cand in candidates {
                        if let Some(parts) = cand.pointer("/content/parts").and_then(Value::as_array) {
                            for part in parts {
                                if let Some(text) = part.get("text").and_then(Value::as_str)
                                    && !text.is_empty() {
                                        yield Ok(StreamEvent::Delta {
                                            content: text.into(),
                                        });
                                    }
                                if let Some(fc) = part.get("functionCall") {
                                    let name = fc
                                        .get("name")
                                        .and_then(Value::as_str)
                                        .unwrap_or_default()
                                        .to_string();
                                    tool_counter += 1;
                                    let id = format!("fc-{tool_counter}");
                                    yield Ok(StreamEvent::ToolCallStart {
                                        id: id.clone(),
                                        name,
                                    });
                                    if let Some(args) = fc.get("args") {
                                        yield Ok(StreamEvent::ToolCallDelta {
                                            id,
                                            chunk: args.to_string(),
                                        });
                                    }
                                }
                            }
                        }
                        if let Some(reason) = cand.get("finishReason").and_then(Value::as_str)
                            && !reason.is_empty() && !final_emitted {
                                yield Ok(StreamEvent::Done {
                                    usage: usage.clone(),
                                });
                                final_emitted = true;
                            }
                    }
                }
            }
        }
        if !final_emitted {
            yield Ok(StreamEvent::Done { usage });
        }
    }
}

fn ingest_usage(target: &mut Usage, v: &Value) {
    if let Some(n) = v.get("promptTokenCount").and_then(Value::as_u64) {
        target.prompt_tokens = u32::try_from(n).unwrap_or(target.prompt_tokens);
    }
    if let Some(n) = v.get("candidatesTokenCount").and_then(Value::as_u64) {
        target.completion_tokens = u32::try_from(n).unwrap_or(target.completion_tokens);
    }
    if let Some(n) = v.get("cachedContentTokenCount").and_then(Value::as_u64) {
        target.cache_read_tokens = Some(u32::try_from(n).unwrap_or(0));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::stream;

    fn lines_to_stream(
        lines: &[&str],
    ) -> impl Stream<Item = Result<Bytes, reqwest::Error>> + use<> {
        let bytes = Bytes::from(lines.join("").into_bytes());
        stream::iter(vec![Ok(bytes)])
    }

    #[tokio::test]
    async fn parses_text_and_usage() {
        let sse = [
            "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"hi\"}]}}]}\n\n",
            "data: {\"candidates\":[{\"content\":{\"parts\":[]},\"finishReason\":\"STOP\"}],\"usageMetadata\":{\"promptTokenCount\":3,\"candidatesTokenCount\":1}}\n\n",
        ];
        let out = parse_sse(lines_to_stream(&sse));
        futures::pin_mut!(out);
        let mut deltas = 0;
        let mut done = None;
        while let Some(ev) = out.next().await {
            match ev.unwrap() {
                StreamEvent::Delta { content } => {
                    assert_eq!(content, "hi");
                    deltas += 1;
                }
                StreamEvent::Done { usage } => {
                    done = Some(usage);
                }
                _ => {}
            }
        }
        assert_eq!(deltas, 1);
        let u = done.unwrap();
        assert_eq!(u.prompt_tokens, 3);
        assert_eq!(u.completion_tokens, 1);
    }

    #[tokio::test]
    async fn parses_function_call() {
        let sse = [
            "data: {\"candidates\":[{\"content\":{\"parts\":[{\"functionCall\":{\"name\":\"bash\",\"args\":{\"command\":\"ls\"}}}]}}]}\n\n",
            "data: {\"candidates\":[{\"content\":{\"parts\":[]},\"finishReason\":\"STOP\"}]}\n\n",
        ];
        let out = parse_sse(lines_to_stream(&sse));
        futures::pin_mut!(out);
        let mut starts = 0;
        let mut deltas = 0;
        while let Some(ev) = out.next().await {
            match ev.unwrap() {
                StreamEvent::ToolCallStart { name, .. } => {
                    assert_eq!(name, "bash");
                    starts += 1;
                }
                StreamEvent::ToolCallDelta { chunk, .. } => {
                    assert!(chunk.contains("command"));
                    deltas += 1;
                }
                _ => {}
            }
        }
        assert_eq!(starts, 1);
        assert_eq!(deltas, 1);
    }
}
