use bytes::Bytes;
use futures::stream::{Stream, StreamExt};
use harness_llm_types::{ProviderError, StreamEvent, Usage};
use serde_json::Value;
use tracing::{debug, warn};

pub fn parse_ndjson<S>(stream: S) -> impl Stream<Item = Result<StreamEvent, ProviderError>> + Send
where
    S: Stream<Item = Result<Bytes, reqwest::Error>> + Send + 'static,
{
    async_stream::stream! {
        futures::pin_mut!(stream);
        let mut buffer = String::new();
        let mut usage = Usage::default();
        let mut tool_counter: u64 = 0;

        while let Some(chunk) = stream.next().await {
            let bytes = match chunk {
                Ok(b) => b,
                Err(e) => {
                    warn!(error = %e, "ollama byte stream error");
                    yield Err(ProviderError::StreamInterrupted);
                    return;
                }
            };
            buffer.push_str(&String::from_utf8_lossy(&bytes));

            while let Some(pos) = buffer.find('\n') {
                let line = buffer[..pos].to_string();
                buffer.drain(..=pos);
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                let json: Value = match serde_json::from_str(line) {
                    Ok(v) => v,
                    Err(e) => {
                        debug!(error = %e, line = %line, "ollama ndjson parse failed");
                        continue;
                    }
                };

                if let Some(n) = json.get("prompt_eval_count").and_then(Value::as_u64) {
                    usage.prompt_tokens = u32::try_from(n).unwrap_or(usage.prompt_tokens);
                }
                if let Some(n) = json.get("eval_count").and_then(Value::as_u64) {
                    usage.completion_tokens = u32::try_from(n).unwrap_or(usage.completion_tokens);
                }

                if let Some(msg) = json.get("message") {
                    if let Some(content) = msg.get("content").and_then(Value::as_str)
                        && !content.is_empty() {
                            yield Ok(StreamEvent::Delta {
                                content: content.into(),
                            });
                        }
                    if let Some(calls) = msg.get("tool_calls").and_then(Value::as_array) {
                        for call in calls {
                            let name = call
                                .pointer("/function/name")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_string();
                            tool_counter += 1;
                            let id = format!("ot-{tool_counter}");
                            yield Ok(StreamEvent::ToolCallStart {
                                id: id.clone(),
                                name,
                            });
                            if let Some(args) = call.pointer("/function/arguments") {
                                yield Ok(StreamEvent::ToolCallDelta {
                                    id,
                                    chunk: args.to_string(),
                                });
                            }
                        }
                    }
                }

                if json.get("done").and_then(Value::as_bool) == Some(true) {
                    yield Ok(StreamEvent::Done {
                        usage: usage.clone(),
                    });
                    return;
                }

                if let Some(err) = json.get("error").and_then(Value::as_str) {
                    yield Ok(StreamEvent::Error {
                        error: ProviderError::ServerError {
                            status: 500,
                            message: err.into(),
                        },
                    });
                }
            }
        }
        yield Ok(StreamEvent::Done { usage });
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
    async fn assembles_text_from_ndjson() {
        let nd = [
            "{\"message\":{\"role\":\"assistant\",\"content\":\"hel\"},\"done\":false}\n",
            "{\"message\":{\"role\":\"assistant\",\"content\":\"lo\"},\"done\":false}\n",
            "{\"done\":true,\"prompt_eval_count\":3,\"eval_count\":2}\n",
        ];
        let out = parse_ndjson(lines_to_stream(&nd));
        futures::pin_mut!(out);
        let mut s = String::new();
        let mut done = None;
        while let Some(ev) = out.next().await {
            match ev.unwrap() {
                StreamEvent::Delta { content } => s.push_str(&content),
                StreamEvent::Done { usage } => done = Some(usage),
                _ => {}
            }
        }
        assert_eq!(s, "hello");
        let u = done.unwrap();
        assert_eq!(u.prompt_tokens, 3);
        assert_eq!(u.completion_tokens, 2);
    }

    #[tokio::test]
    async fn parses_tool_calls() {
        let nd = [
            "{\"message\":{\"role\":\"assistant\",\"content\":\"\",\"tool_calls\":[{\"function\":{\"name\":\"bash\",\"arguments\":{\"cmd\":\"ls\"}}}]},\"done\":false}\n",
            "{\"done\":true}\n",
        ];
        let out = parse_ndjson(lines_to_stream(&nd));
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
                    assert!(chunk.contains("cmd"));
                    deltas += 1;
                }
                _ => {}
            }
        }
        assert_eq!(starts, 1);
        assert_eq!(deltas, 1);
    }

    #[tokio::test]
    async fn error_becomes_event() {
        let nd = ["{\"error\":\"no such model\"}\n"];
        let out = parse_ndjson(lines_to_stream(&nd));
        futures::pin_mut!(out);
        let mut errs = 0;
        while let Some(ev) = out.next().await {
            if let StreamEvent::Error { .. } = ev.unwrap() {
                errs += 1;
            }
        }
        assert_eq!(errs, 1);
    }
}
