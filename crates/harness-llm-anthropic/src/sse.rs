use bytes::Bytes;
use eventsource_stream::Eventsource;
use futures::stream::{Stream, StreamExt};
use harness_llm_types::{ProviderError, StreamEvent, Usage};
use serde_json::Value;
use tracing::{debug, warn};

pub fn parse_sse<S>(stream: S) -> impl Stream<Item = Result<StreamEvent, ProviderError>> + Send
where
    S: Stream<Item = Result<Bytes, reqwest::Error>> + Send + 'static,
{
    let events = stream.eventsource();

    async_stream::stream! {
        let mut usage = Usage::default();
        let mut active_tool_id: Option<String> = None;
        futures::pin_mut!(events);
        while let Some(ev) = events.next().await {
            let event = match ev {
                Ok(e) => e,
                Err(e) => {
                    warn!(error = %e, "sse parse error");
                    yield Err(ProviderError::StreamInterrupted);
                    return;
                }
            };

            let data = event.data;
            if data.is_empty() || data == "[DONE]" {
                continue;
            }

            let json: Value = match serde_json::from_str(&data) {
                Ok(v) => v,
                Err(e) => {
                    debug!(error = %e, data = %data, "sse json parse failed");
                    continue;
                }
            };

            let kind = json.get("type").and_then(Value::as_str).unwrap_or("");

            match kind {
                "message_start" => {
                    if let Some(u) = json.pointer("/message/usage") {
                        ingest_usage(&mut usage, u);
                    }
                }
                "content_block_start" => {
                    if json.pointer("/content_block/type").and_then(Value::as_str)
                        == Some("tool_use")
                    {
                        let id = json
                            .pointer("/content_block/id")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string();
                        let name = json
                            .pointer("/content_block/name")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string();
                        active_tool_id = Some(id.clone());
                        yield Ok(StreamEvent::ToolCallStart { id, name });
                    }
                }
                "content_block_delta" => {
                    let delta_type = json
                        .pointer("/delta/type")
                        .and_then(Value::as_str)
                        .unwrap_or("");
                    match delta_type {
                        "text_delta" => {
                            if let Some(text) =
                                json.pointer("/delta/text").and_then(Value::as_str)
                            {
                                yield Ok(StreamEvent::Delta {
                                    content: text.to_string(),
                                });
                            }
                        }
                        "input_json_delta" => {
                            if let (Some(id), Some(chunk)) = (
                                active_tool_id.clone(),
                                json.pointer("/delta/partial_json")
                                    .and_then(Value::as_str)
                                    .map(str::to_string),
                            ) {
                                yield Ok(StreamEvent::ToolCallDelta { id, chunk });
                            }
                        }
                        _ => {}
                    }
                }
                "content_block_stop" => {
                    active_tool_id = None;
                }
                "message_delta" => {
                    if let Some(u) = json.get("usage") {
                        ingest_usage(&mut usage, u);
                    }
                }
                "message_stop" => {
                    yield Ok(StreamEvent::Done {
                        usage: usage.clone(),
                    });
                }
                "error" => {
                    let message = json
                        .pointer("/error/message")
                        .and_then(Value::as_str)
                        .unwrap_or("stream error")
                        .to_string();
                    yield Ok(StreamEvent::Error {
                        error: ProviderError::ServerError {
                            status: 500,
                            message,
                        },
                    });
                }
                _ => {}
            }
        }
    }
}

fn ingest_usage(target: &mut Usage, v: &Value) {
    if let Some(n) = v.get("input_tokens").and_then(Value::as_u64) {
        target.prompt_tokens = u32::try_from(n).unwrap_or(target.prompt_tokens);
    }
    if let Some(n) = v.get("output_tokens").and_then(Value::as_u64) {
        target.completion_tokens = u32::try_from(n).unwrap_or(target.completion_tokens);
    }
    if let Some(n) = v.get("cache_read_input_tokens").and_then(Value::as_u64) {
        target.cache_read_tokens = Some(u32::try_from(n).unwrap_or(0));
    }
    if let Some(n) = v.get("cache_creation_input_tokens").and_then(Value::as_u64) {
        target.cache_creation_tokens = Some(u32::try_from(n).unwrap_or(0));
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
    async fn parses_text_deltas() {
        let sse = [
            "event: content_block_start\n",
            "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
            "event: content_block_delta\n",
            "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"hi\"}}\n\n",
            "event: message_stop\n",
            "data: {\"type\":\"message_stop\"}\n\n",
        ];

        let out = parse_sse(lines_to_stream(&sse));
        futures::pin_mut!(out);
        let mut deltas = 0;
        let mut dones = 0;
        while let Some(ev) = out.next().await {
            match ev.unwrap() {
                StreamEvent::Delta { content } => {
                    assert_eq!(content, "hi");
                    deltas += 1;
                }
                StreamEvent::Done { .. } => dones += 1,
                _ => {}
            }
        }
        assert_eq!(deltas, 1);
        assert_eq!(dones, 1);
    }

    #[tokio::test]
    async fn parses_tool_call_start_and_delta() {
        let sse = [
            "event: content_block_start\n",
            "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"tool_use\",\"id\":\"t1\",\"name\":\"bash\",\"input\":{}}}\n\n",
            "event: content_block_delta\n",
            "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"cmd\\\":\"}}\n\n",
            "event: content_block_stop\n",
            "data: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
            "event: message_stop\n",
            "data: {\"type\":\"message_stop\"}\n\n",
        ];

        let out = parse_sse(lines_to_stream(&sse));
        futures::pin_mut!(out);
        let mut starts = 0;
        let mut deltas = 0;
        while let Some(ev) = out.next().await {
            match ev.unwrap() {
                StreamEvent::ToolCallStart { id, name } => {
                    assert_eq!(id, "t1");
                    assert_eq!(name, "bash");
                    starts += 1;
                }
                StreamEvent::ToolCallDelta { id, chunk } => {
                    assert_eq!(id, "t1");
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
    async fn ignores_unrelated_event_types() {
        let sse = [
            "event: ping\n",
            "data: {\"type\":\"ping\"}\n\n",
            "event: message_stop\n",
            "data: {\"type\":\"message_stop\"}\n\n",
        ];
        let out = parse_sse(lines_to_stream(&sse));
        futures::pin_mut!(out);
        let mut count = 0;
        while let Some(ev) = out.next().await {
            let ev = ev.unwrap();
            if matches!(ev, StreamEvent::Done { .. }) {
                count += 1;
            }
        }
        assert_eq!(count, 1);
    }
}
