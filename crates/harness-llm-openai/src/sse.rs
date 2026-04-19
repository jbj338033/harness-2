use bytes::Bytes;
use eventsource_stream::Eventsource;
use futures::stream::{Stream, StreamExt};
use harness_llm_types::{ProviderError, StreamEvent, Usage};
use serde_json::Value;
use std::collections::HashMap;
use tracing::{debug, warn};

pub fn parse_sse<S>(stream: S) -> impl Stream<Item = Result<StreamEvent, ProviderError>> + Send
where
    S: Stream<Item = Result<Bytes, reqwest::Error>> + Send + 'static,
{
    let events = stream.eventsource();
    async_stream::stream! {
        let mut usage = Usage::default();
        let mut tool_ids: HashMap<u64, String> = HashMap::new();
        futures::pin_mut!(events);
        let mut done_emitted = false;
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
            if data.is_empty() {
                continue;
            }
            if data == "[DONE]" {
                if !done_emitted {
                    yield Ok(StreamEvent::Done { usage: usage.clone() });
                }
                return;
            }

            let json: Value = match serde_json::from_str(&data) {
                Ok(v) => v,
                Err(e) => {
                    debug!(error = %e, data = %data, "sse json parse failed");
                    continue;
                }
            };

            if let Some(u) = json.get("usage") {
                ingest_usage(&mut usage, u);
            }

            let Some(choices) = json.get("choices").and_then(Value::as_array) else {
                continue;
            };
            for choice in choices {
                let Some(delta) = choice.get("delta") else {
                    continue;
                };
                if let Some(text) = delta.get("content").and_then(Value::as_str)
                    && !text.is_empty() {
                        yield Ok(StreamEvent::Delta { content: text.into() });
                    }
                if let Some(calls) = delta.get("tool_calls").and_then(Value::as_array) {
                    for call in calls {
                        let index = call.get("index").and_then(Value::as_u64).unwrap_or(0);
                        let id_str = call.get("id").and_then(Value::as_str).map(str::to_string);
                        let name = call
                            .pointer("/function/name")
                            .and_then(Value::as_str)
                            .map(str::to_string);
                        let args = call
                            .pointer("/function/arguments")
                            .and_then(Value::as_str)
                            .map(str::to_string);

                        if let Some(id) = id_str.clone() {
                            tool_ids.insert(index, id.clone());
                            if let Some(n) = name.clone() {
                                yield Ok(StreamEvent::ToolCallStart { id, name: n });
                            }
                        } else if let Some(n) = name.clone()
                            && let Some(existing) = tool_ids.get(&index).cloned()
                        {
                            yield Ok(StreamEvent::ToolCallStart { id: existing, name: n });
                        }

                        if let Some(chunk) = args
                            && !chunk.is_empty() {
                                let id = tool_ids
                                    .get(&index)
                                    .cloned()
                                    .unwrap_or_else(|| format!("t{index}"));
                                yield Ok(StreamEvent::ToolCallDelta { id, chunk });
                            }
                    }
                }
                if let Some(reason) = choice.get("finish_reason").and_then(Value::as_str)
                    && !reason.is_empty() {
                        yield Ok(StreamEvent::Done { usage: usage.clone() });
                        done_emitted = true;
                    }
            }
        }
        if !done_emitted {
            yield Ok(StreamEvent::Done { usage });
        }
    }
}

fn ingest_usage(target: &mut Usage, v: &Value) {
    if let Some(n) = v.get("prompt_tokens").and_then(Value::as_u64) {
        target.prompt_tokens = u32::try_from(n).unwrap_or(target.prompt_tokens);
    }
    if let Some(n) = v.get("completion_tokens").and_then(Value::as_u64) {
        target.completion_tokens = u32::try_from(n).unwrap_or(target.completion_tokens);
    }
    if let Some(n) = v
        .pointer("/prompt_tokens_details/cached_tokens")
        .and_then(Value::as_u64)
    {
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
    async fn parses_text_deltas_and_done() {
        let sse = [
            "data: {\"choices\":[{\"delta\":{\"content\":\"hi\"}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":4,\"completion_tokens\":1}}\n\n",
            "data: [DONE]\n\n",
        ];
        let out = parse_sse(lines_to_stream(&sse));
        futures::pin_mut!(out);
        let mut deltas = 0;
        let mut dones = 0;
        let mut last_usage = Usage::default();
        while let Some(ev) = out.next().await {
            match ev.unwrap() {
                StreamEvent::Delta { content } => {
                    assert_eq!(content, "hi");
                    deltas += 1;
                }
                StreamEvent::Done { usage } => {
                    dones += 1;
                    last_usage = usage;
                }
                _ => {}
            }
        }
        assert_eq!(deltas, 1);
        assert!(dones >= 1);
        assert_eq!(last_usage.prompt_tokens, 4);
        assert_eq!(last_usage.completion_tokens, 1);
    }

    #[tokio::test]
    async fn parses_tool_call_start_and_delta() {
        let sse = [
            "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"t1\",\"type\":\"function\",\"function\":{\"name\":\"bash\",\"arguments\":\"\"}}]}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"{\\\"cmd\\\":\"}}]}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"tool_calls\"}]}\n\n",
            "data: [DONE]\n\n",
        ];
        let out = parse_sse(lines_to_stream(&sse));
        futures::pin_mut!(out);
        let mut starts = 0;
        let mut call_deltas = 0;
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
                    call_deltas += 1;
                }
                _ => {}
            }
        }
        assert_eq!(starts, 1);
        assert_eq!(call_deltas, 1);
    }

    #[tokio::test]
    async fn skips_empty_content_chunks() {
        let sse = [
            "data: {\"choices\":[{\"delta\":{\"role\":\"assistant\"}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"content\":\"\"}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"content\":\"x\"}}]}\n\n",
            "data: [DONE]\n\n",
        ];
        let out = parse_sse(lines_to_stream(&sse));
        futures::pin_mut!(out);
        let mut count = 0;
        while let Some(ev) = out.next().await {
            if let StreamEvent::Delta { content } = ev.unwrap() {
                assert_eq!(content, "x");
                count += 1;
            }
        }
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn done_emitted_even_without_finish_reason() {
        let sse = [
            "data: {\"choices\":[{\"delta\":{\"content\":\"x\"}}]}\n\n",
            "data: [DONE]\n\n",
        ];
        let out = parse_sse(lines_to_stream(&sse));
        futures::pin_mut!(out);
        let mut dones = 0;
        while let Some(ev) = out.next().await {
            if let StreamEvent::Done { .. } = ev.unwrap() {
                dones += 1;
            }
        }
        assert_eq!(dones, 1);
    }
}
