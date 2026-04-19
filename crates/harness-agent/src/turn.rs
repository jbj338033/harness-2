use crate::flush::Flusher;
use futures::StreamExt;
use harness_core::{AgentId, MessageId, SessionId};
use harness_llm::ProviderPool;
use harness_llm_types::{
    ChatOptions, ChatRequest, ContentBlock, Message, MessageRole as LlmMessageRole, StreamEvent,
    ToolDef,
};
use harness_session::{
    agent::{self, AgentStatus},
    broadcast::{SessionBroadcaster, SessionEvent},
    message::{self, MessageRole, NewMessage},
    tool_call,
};
use harness_storage::WriterHandle;
use harness_tools::{Registry, ToolContext, ToolOutput};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant as StdInstant;
use thiserror::Error;
use tokio::time::{Instant, sleep};
use tracing::{info, warn};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TurnOutcome {
    Done { message_id: MessageId },
    Failed { reason: String },
}

#[derive(Debug, Error)]
pub enum TurnError {
    #[error("session: {0}")]
    Session(#[from] harness_session::SessionError),
}

pub struct TurnInputs<'a> {
    pub session_id: SessionId,
    pub agent_id: AgentId,
    pub model: String,
    pub family: String,
    pub user_message: String,

    pub system_prompt: String,
    pub tool_defs: Vec<ToolDef>,
    pub prior_messages: Vec<Message>,

    pub pool: Arc<ProviderPool>,
    pub tool_registry: Arc<Registry>,
    pub tool_context: ToolContext,

    pub writer: &'a WriterHandle,
    pub broadcaster: &'a Arc<SessionBroadcaster>,

    pub max_iterations: usize,
}

pub const DEFAULT_MAX_ITERATIONS: usize = 8;

pub async fn run_turn(inputs: TurnInputs<'_>) -> Result<TurnOutcome, TurnError> {
    let TurnInputs {
        session_id,
        agent_id,
        model,
        family,
        user_message,
        system_prompt,
        tool_defs,
        prior_messages,
        pool,
        tool_registry,
        tool_context,
        writer,
        broadcaster,
        max_iterations,
    } = inputs;

    let max_iterations = max_iterations.max(1);

    let (user_mid, _) = message::insert(
        writer,
        NewMessage {
            agent_id,
            role: MessageRole::User,
            content: Some(user_message.clone()),
            model: None,
            kind: harness_session::message::MessageKind::Chat,
        },
    )
    .await?;
    broadcaster.publish(
        session_id,
        SessionEvent::MessageCreated {
            agent_id,
            message_id: user_mid,
            role: "user".into(),
        },
    );

    agent::set_status(writer, agent_id, AgentStatus::Running).await?;
    broadcaster.publish(
        session_id,
        SessionEvent::AgentStatus {
            agent_id,
            status: AgentStatus::Running.as_str().into(),
        },
    );

    let mut messages = prior_messages;
    messages.push(Message {
        role: LlmMessageRole::User,
        content: vec![ContentBlock::Text { text: user_message }],
    });

    for iteration in 0..max_iterations {
        let (assistant_mid, _) = message::insert(
            writer,
            NewMessage {
                agent_id,
                role: MessageRole::Assistant,
                content: Some(String::new()),
                model: Some(model.clone()),
                kind: harness_session::message::MessageKind::Chat,
            },
        )
        .await?;
        broadcaster.publish(
            session_id,
            SessionEvent::MessageCreated {
                agent_id,
                message_id: assistant_mid,
                role: "assistant".into(),
            },
        );

        let request = ChatRequest {
            system: Some(system_prompt.clone()),
            messages: messages.clone(),
            tools: tool_defs.clone(),
        };

        let mut stream = match pool
            .chat(&family, &model, request, ChatOptions::default())
            .await
        {
            Ok(s) => s,
            Err(e) => {
                let reason = format!("provider pool: {e}");
                finalize_failure(writer, broadcaster, session_id, agent_id, &reason).await?;
                return Ok(TurnOutcome::Failed { reason });
            }
        };

        let flusher = Flusher::new(writer.clone(), assistant_mid);
        let mut call_buffers: HashMap<String, ToolCallBuffer> = HashMap::new();
        let mut ordered_ids: Vec<String> = Vec::new();
        let mut assistant_text = String::new();
        let start = Instant::now();
        let mut last_flush = start;
        let mut failure: Option<String> = None;

        loop {
            let tick_deadline = last_flush + crate::FLUSH_INTERVAL;
            let now_instant = Instant::now();
            let until_tick = tick_deadline.saturating_duration_since(now_instant);

            tokio::select! {
                biased;
                ev = stream.next() => {
                    let Some(ev) = ev else {
                        failure = Some("stream closed without completion".into());
                        break;
                    };
                    match ev {
                        Ok(StreamEvent::Delta { content }) => {
                            assistant_text.push_str(&content);
                            flusher.push(&content).await;
                            broadcaster.publish(
                                session_id,
                                SessionEvent::MessageDelta {
                                    message_id: assistant_mid,
                                    content,
                                },
                            );
                        }
                        Ok(StreamEvent::ToolCallStart { id, name }) => {
                            if !call_buffers.contains_key(&id) {
                                ordered_ids.push(id.clone());
                            }
                            call_buffers
                                .entry(id)
                                .and_modify(|b| b.name.clone_from(&name))
                                .or_insert_with(|| ToolCallBuffer::new(name));
                        }
                        Ok(StreamEvent::ToolCallDelta { id, chunk }) => {
                            if !call_buffers.contains_key(&id) {
                                ordered_ids.push(id.clone());
                            }
                            call_buffers
                                .entry(id)
                                .or_default()
                                .input_json
                                .push_str(&chunk);
                        }
                        Ok(StreamEvent::Done { usage }) => {
                            flusher
                                .set_usage(
                                    Some(i64::from(usage.prompt_tokens)),
                                    Some(i64::from(usage.completion_tokens)),
                                    usage.cost,
                                )
                                .await;
                            break;
                        }
                        Ok(StreamEvent::Error { error }) => {
                            failure = Some(error.to_string());
                            break;
                        }
                        Err(e) => {
                            failure = Some(e.to_string());
                            break;
                        }
                    }
                }
                () = sleep(until_tick) => {
                    if flusher.should_flush(Instant::now()).await {
                        if let Err(e) = flusher.flush().await {
                            warn!(error = %e, "intermediate flush failed");
                        }
                        last_flush = Instant::now();
                    }
                }
            }
        }

        if let Err(e) = flusher.finish().await {
            warn!(error = %e, "final flush failed");
        }

        if let Some(reason) = failure {
            finalize_failure(writer, broadcaster, session_id, agent_id, &reason).await?;
            return Ok(TurnOutcome::Failed { reason });
        }

        let assistant_content =
            build_assistant_content(&assistant_text, &ordered_ids, &call_buffers);
        messages.push(Message {
            role: LlmMessageRole::Assistant,
            content: assistant_content,
        });

        if ordered_ids.is_empty() {
            broadcaster.publish(
                session_id,
                SessionEvent::MessageDone {
                    message_id: assistant_mid,
                },
            );
            agent::set_status(writer, agent_id, AgentStatus::Done).await?;
            broadcaster.publish(
                session_id,
                SessionEvent::AgentStatus {
                    agent_id,
                    status: AgentStatus::Done.as_str().into(),
                },
            );
            info!(agent = %agent_id, "turn complete");
            return Ok(TurnOutcome::Done {
                message_id: assistant_mid,
            });
        }

        let mut tool_result_blocks: Vec<ContentBlock> = Vec::with_capacity(ordered_ids.len());
        for call_id in &ordered_ids {
            let buf = call_buffers.get(call_id).expect("buffer invariant");
            let input = parse_input(&buf.input_json);
            let persisted_id =
                tool_call::insert_pending(writer, assistant_mid, buf.name.clone(), input.clone())
                    .await?;
            broadcaster.publish(
                session_id,
                SessionEvent::ToolCallStart {
                    message_id: assistant_mid,
                    tool_call_id: persisted_id,
                    name: buf.name.clone(),
                    input_preview: preview_value(&input),
                },
            );

            let (output, is_error, duration_ms) =
                execute_tool(&tool_registry, &tool_context, &buf.name, input.clone()).await;

            tool_call::record_result(
                writer,
                persisted_id,
                output.clone(),
                Some(i64::from(is_error)),
                Some(duration_ms),
            )
            .await?;
            broadcaster.publish(
                session_id,
                SessionEvent::ToolCallResult {
                    tool_call_id: persisted_id,
                    output: output.clone(),
                    is_error,
                },
            );

            tool_result_blocks.push(ContentBlock::ToolResult {
                id: call_id.clone(),
                output,
                is_error,
            });
        }
        broadcaster.publish(
            session_id,
            SessionEvent::MessageDone {
                message_id: assistant_mid,
            },
        );

        messages.push(Message {
            role: LlmMessageRole::User,
            content: tool_result_blocks,
        });

        if iteration + 1 == max_iterations {
            let reason = format!("tool-call budget exhausted after {max_iterations} iterations");
            finalize_failure(writer, broadcaster, session_id, agent_id, &reason).await?;
            return Ok(TurnOutcome::Failed { reason });
        }
    }

    let reason = "turn terminated without an outcome".to_string();
    finalize_failure(writer, broadcaster, session_id, agent_id, &reason).await?;
    Ok(TurnOutcome::Failed { reason })
}

async fn finalize_failure(
    writer: &WriterHandle,
    broadcaster: &Arc<SessionBroadcaster>,
    session_id: SessionId,
    agent_id: AgentId,
    reason: &str,
) -> Result<(), TurnError> {
    agent::set_status(writer, agent_id, AgentStatus::Failed).await?;
    broadcaster.publish(
        session_id,
        SessionEvent::AgentStatus {
            agent_id,
            status: AgentStatus::Failed.as_str().into(),
        },
    );
    broadcaster.publish(
        session_id,
        SessionEvent::Error {
            reason: reason.into(),
        },
    );
    warn!(agent = %agent_id, %reason, "turn failed");
    Ok(())
}

#[derive(Default, Debug)]
struct ToolCallBuffer {
    name: String,
    input_json: String,
}

impl ToolCallBuffer {
    fn new(name: String) -> Self {
        Self {
            name,
            input_json: String::new(),
        }
    }
}

fn build_assistant_content(
    text: &str,
    ordered_ids: &[String],
    buffers: &HashMap<String, ToolCallBuffer>,
) -> Vec<ContentBlock> {
    let mut blocks: Vec<ContentBlock> = Vec::new();
    if !text.is_empty() {
        blocks.push(ContentBlock::Text {
            text: text.to_string(),
        });
    }
    for id in ordered_ids {
        let Some(buf) = buffers.get(id) else { continue };
        let input = parse_input(&buf.input_json);
        blocks.push(ContentBlock::ToolCall {
            id: id.clone(),
            name: buf.name.clone(),
            input,
        });
    }
    blocks
}

fn parse_input(raw: &str) -> Value {
    if raw.is_empty() {
        return Value::Object(serde_json::Map::new());
    }
    serde_json::from_str(raw).unwrap_or(Value::String(raw.into()))
}

fn preview_value(v: &Value) -> String {
    let s = v.to_string();
    if s.len() > 160 {
        format!("{}…", &s[..160])
    } else {
        s
    }
}

async fn execute_tool(
    registry: &Registry,
    ctx: &ToolContext,
    name: &str,
    input: Value,
) -> (String, bool, i64) {
    let start = StdInstant::now();
    let output: ToolOutput = match registry.get(name) {
        Some(tool) => match tool.execute(input, ctx).await {
            Ok(out) => out,
            Err(err) => ToolOutput::err(err.to_string()),
        },
        None => ToolOutput::err(format!("unknown tool: {name}")),
    };
    let duration_ms = i64::try_from(start.elapsed().as_millis()).unwrap_or(i64::MAX);
    (output.content, output.is_error, duration_ms)
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use futures::stream;
    use harness_llm::{BoxStream, Provider, ProviderSlot};
    use harness_llm_types::{ProviderError, Usage};
    use harness_session::{agent::NewAgent, manager::SessionManager};
    use harness_storage::{Database, Writer};
    use harness_tools::{Tool, ToolError};
    use serde_json::json;
    use std::sync::Mutex;
    use tempfile::NamedTempFile;

    struct ScriptedProvider {
        scripts: Mutex<Vec<Vec<StreamEvent>>>,
    }

    impl ScriptedProvider {
        fn new(scripts: Vec<Vec<StreamEvent>>) -> Self {
            Self {
                scripts: Mutex::new(scripts),
            }
        }
    }

    #[async_trait]
    impl Provider for ScriptedProvider {
        fn id(&self) -> &'static str {
            "scripted"
        }
        fn family(&self) -> &'static str {
            "test"
        }
        async fn list_models(&self) -> Result<Vec<String>, ProviderError> {
            Ok(vec!["mock".into()])
        }
        async fn chat(
            &self,
            _model: &str,
            _request: ChatRequest,
            _options: ChatOptions,
        ) -> Result<BoxStream, ProviderError> {
            let events = {
                let mut s = self.scripts.lock().expect("mutex poisoned");
                if s.is_empty() {
                    return Err(ProviderError::StreamInterrupted);
                }
                s.remove(0)
            };
            let stream = stream::iter(events.into_iter().map(Ok::<_, ProviderError>));
            Ok(Box::pin(stream))
        }
    }

    struct EchoTool;
    #[async_trait]
    impl Tool for EchoTool {
        fn name(&self) -> &'static str {
            "echo"
        }
        fn description(&self) -> &'static str {
            "echo input.text back"
        }
        fn input_schema(&self) -> Value {
            json!({"type": "object"})
        }
        async fn execute(&self, input: Value, _ctx: &ToolContext) -> Result<ToolOutput, ToolError> {
            let text = input.get("text").and_then(Value::as_str).unwrap_or("");
            Ok(ToolOutput::ok(format!("echoed: {text}")))
        }
    }

    async fn setup() -> (
        NamedTempFile,
        WriterHandle,
        Arc<SessionBroadcaster>,
        SessionId,
        AgentId,
    ) {
        let f = NamedTempFile::new().unwrap();
        Database::open(f.path()).unwrap();
        let writer = Writer::spawn(f.path()).unwrap();
        let sm = SessionManager::new(&writer);
        let s = sm.create("/tmp", None).await.unwrap();
        let aid = agent::insert(
            &writer,
            NewAgent {
                session_id: s.id,
                parent_id: None,
                role: "root".into(),
                model: "mock".into(),
                system_prompt: None,
                worktree_path: None,
                wave: None,
            },
        )
        .await
        .unwrap();
        let b = Arc::new(SessionBroadcaster::default());
        (f, writer, b, s.id, aid)
    }

    fn pool_for(scripts: Vec<Vec<StreamEvent>>) -> Arc<ProviderPool> {
        Arc::new(ProviderPool::new(vec![ProviderSlot::new(Arc::new(
            ScriptedProvider::new(scripts),
        ))]))
    }

    fn empty_registry() -> Arc<Registry> {
        Arc::new(Registry::new())
    }

    fn ctx() -> ToolContext {
        ToolContext::test("/tmp")
    }

    #[tokio::test]
    async fn happy_path_streams_and_completes() {
        let (f, writer, b, sid, aid) = setup().await;
        let pool = pool_for(vec![vec![
            StreamEvent::Delta {
                content: "hello ".into(),
            },
            StreamEvent::Delta {
                content: "world".into(),
            },
            StreamEvent::Done {
                usage: Usage {
                    prompt_tokens: 3,
                    completion_tokens: 2,
                    ..Default::default()
                },
            },
        ]]);

        let outcome = run_turn(TurnInputs {
            session_id: sid,
            agent_id: aid,
            model: "mock".into(),
            family: "test".into(),
            user_message: "hi".into(),
            system_prompt: "sys".into(),
            tool_defs: vec![],
            prior_messages: vec![],
            pool,
            tool_registry: empty_registry(),
            tool_context: ctx(),
            writer: &writer,
            broadcaster: &b,
            max_iterations: DEFAULT_MAX_ITERATIONS,
        })
        .await
        .unwrap();

        let mid = match outcome {
            TurnOutcome::Done { message_id } => message_id,
            TurnOutcome::Failed { reason } => panic!("failed: {reason}"),
        };

        let reader = rusqlite::Connection::open(f.path()).unwrap();
        let msgs = message::list_for_agent(&reader, aid).unwrap();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].role, MessageRole::User);
        assert_eq!(msgs[1].role, MessageRole::Assistant);
        assert_eq!(msgs[1].content.as_deref(), Some("hello world"));
        assert_eq!(msgs[1].id, mid);
        assert_eq!(msgs[1].tokens_out, Some(2));
    }

    #[tokio::test]
    async fn stream_error_marks_agent_failed() {
        let (f, writer, b, sid, aid) = setup().await;
        let pool = pool_for(vec![vec![
            StreamEvent::Delta {
                content: "partial".into(),
            },
            StreamEvent::Error {
                error: ProviderError::StreamInterrupted,
            },
        ]]);

        let outcome = run_turn(TurnInputs {
            session_id: sid,
            agent_id: aid,
            model: "mock".into(),
            family: "test".into(),
            user_message: "hi".into(),
            system_prompt: "sys".into(),
            tool_defs: vec![],
            prior_messages: vec![],
            pool,
            tool_registry: empty_registry(),
            tool_context: ctx(),
            writer: &writer,
            broadcaster: &b,
            max_iterations: DEFAULT_MAX_ITERATIONS,
        })
        .await
        .unwrap();

        assert!(matches!(outcome, TurnOutcome::Failed { .. }));
        let reader = rusqlite::Connection::open(f.path()).unwrap();
        let agents = agent::list_for_session(&reader, sid).unwrap();
        assert_eq!(agents[0].status, AgentStatus::Failed);
        let msgs = message::list_for_agent(&reader, aid).unwrap();
        assert_eq!(msgs[1].content.as_deref(), Some("partial"));
    }

    #[tokio::test]
    async fn stream_without_done_is_treated_as_failure() {
        let (_f, writer, b, sid, aid) = setup().await;
        let pool = pool_for(vec![vec![StreamEvent::Delta {
            content: "x".into(),
        }]]);

        let outcome = run_turn(TurnInputs {
            session_id: sid,
            agent_id: aid,
            model: "mock".into(),
            family: "test".into(),
            user_message: "hi".into(),
            system_prompt: "sys".into(),
            tool_defs: vec![],
            prior_messages: vec![],
            pool,
            tool_registry: empty_registry(),
            tool_context: ctx(),
            writer: &writer,
            broadcaster: &b,
            max_iterations: DEFAULT_MAX_ITERATIONS,
        })
        .await
        .unwrap();

        assert!(matches!(outcome, TurnOutcome::Failed { .. }));
    }

    #[tokio::test]
    async fn tool_call_is_dispatched_and_result_appended() {
        let (f, writer, b, sid, aid) = setup().await;
        let pool = pool_for(vec![
            vec![
                StreamEvent::ToolCallStart {
                    id: "call-1".into(),
                    name: "echo".into(),
                },
                StreamEvent::ToolCallDelta {
                    id: "call-1".into(),
                    chunk: r#"{"text":"hi"}"#.into(),
                },
                StreamEvent::Done {
                    usage: Usage::default(),
                },
            ],
            vec![
                StreamEvent::Delta {
                    content: "done".into(),
                },
                StreamEvent::Done {
                    usage: Usage::default(),
                },
            ],
        ]);

        let registry = Registry::new();
        registry.register(Arc::new(EchoTool));
        let registry = Arc::new(registry);

        let outcome = run_turn(TurnInputs {
            session_id: sid,
            agent_id: aid,
            model: "mock".into(),
            family: "test".into(),
            user_message: "run echo".into(),
            system_prompt: "sys".into(),
            tool_defs: vec![ToolDef {
                name: "echo".into(),
                description: "echo".into(),
                input_schema: json!({}),
            }],
            prior_messages: vec![],
            pool,
            tool_registry: registry,
            tool_context: ctx(),
            writer: &writer,
            broadcaster: &b,
            max_iterations: DEFAULT_MAX_ITERATIONS,
        })
        .await
        .unwrap();

        let final_mid = match outcome {
            TurnOutcome::Done { message_id } => message_id,
            TurnOutcome::Failed { reason } => panic!("failed: {reason}"),
        };

        let reader = rusqlite::Connection::open(f.path()).unwrap();
        let msgs = message::list_for_agent(&reader, aid).unwrap();
        assert_eq!(
            msgs.len(),
            3,
            "user + tool-only assistant + final assistant"
        );
        assert_eq!(msgs[2].id, final_mid);
        assert_eq!(msgs[2].content.as_deref(), Some("done"));
        assert_eq!(msgs[1].content.as_deref(), Some(""));

        let tool_rows = tool_call::list_for_message(&reader, msgs[1].id).unwrap();
        assert_eq!(tool_rows.len(), 1);
        assert_eq!(tool_rows[0].name, "echo");
        assert_eq!(tool_rows[0].output.as_deref(), Some("echoed: hi"));
        assert_eq!(tool_rows[0].exit_code, Some(0));
    }

    #[tokio::test]
    async fn unknown_tool_returns_error_block_and_continues() {
        let (f, writer, b, sid, aid) = setup().await;
        let pool = pool_for(vec![
            vec![
                StreamEvent::ToolCallStart {
                    id: "x".into(),
                    name: "missing".into(),
                },
                StreamEvent::ToolCallDelta {
                    id: "x".into(),
                    chunk: "{}".into(),
                },
                StreamEvent::Done {
                    usage: Usage::default(),
                },
            ],
            vec![
                StreamEvent::Delta {
                    content: "ok".into(),
                },
                StreamEvent::Done {
                    usage: Usage::default(),
                },
            ],
        ]);

        let outcome = run_turn(TurnInputs {
            session_id: sid,
            agent_id: aid,
            model: "mock".into(),
            family: "test".into(),
            user_message: "go".into(),
            system_prompt: "sys".into(),
            tool_defs: vec![],
            prior_messages: vec![],
            pool,
            tool_registry: empty_registry(),
            tool_context: ctx(),
            writer: &writer,
            broadcaster: &b,
            max_iterations: DEFAULT_MAX_ITERATIONS,
        })
        .await
        .unwrap();

        assert!(matches!(outcome, TurnOutcome::Done { .. }));
        let reader = rusqlite::Connection::open(f.path()).unwrap();
        let msgs = message::list_for_agent(&reader, aid).unwrap();
        let rows = tool_call::list_for_message(&reader, msgs[1].id).unwrap();
        assert_eq!(rows[0].exit_code, Some(1));
        assert!(rows[0].output.as_deref().unwrap().contains("unknown tool"));
    }

    #[tokio::test]
    async fn max_iterations_limit_marks_failed() {
        let (f, writer, b, sid, aid) = setup().await;
        let script: Vec<StreamEvent> = vec![
            StreamEvent::ToolCallStart {
                id: "loop".into(),
                name: "echo".into(),
            },
            StreamEvent::ToolCallDelta {
                id: "loop".into(),
                chunk: "{}".into(),
            },
            StreamEvent::Done {
                usage: Usage::default(),
            },
        ];
        let pool = pool_for(vec![script.clone(), script.clone()]);
        let registry = Registry::new();
        registry.register(Arc::new(EchoTool));
        let registry = Arc::new(registry);

        let outcome = run_turn(TurnInputs {
            session_id: sid,
            agent_id: aid,
            model: "mock".into(),
            family: "test".into(),
            user_message: "keep going".into(),
            system_prompt: "sys".into(),
            tool_defs: vec![],
            prior_messages: vec![],
            pool,
            tool_registry: registry,
            tool_context: ctx(),
            writer: &writer,
            broadcaster: &b,
            max_iterations: 2,
        })
        .await
        .unwrap();

        match outcome {
            TurnOutcome::Failed { reason } => assert!(reason.contains("budget")),
            TurnOutcome::Done { .. } => panic!("expected failure"),
        }
        let reader = rusqlite::Connection::open(f.path()).unwrap();
        let agents = agent::list_for_session(&reader, sid).unwrap();
        assert_eq!(agents[0].status, AgentStatus::Failed);
    }
}
