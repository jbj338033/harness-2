use crate::Daemon;
use crate::approval_requester::DaemonApprovalRequester;
use crate::rpc::errors::{map_session_err, rpc_err};
use crate::rpc::events::event_to_notification;
use harness_agent::turn::{DEFAULT_MAX_ITERATIONS, TurnInputs, TurnOutcome, run_turn};
use harness_context::{AssemblyInputs, assemble};
use harness_core::{AgentId, SessionId};
use harness_llm_types::{ContentBlock, Message as LlmMessage, MessageRole as LlmRole, ToolDef};
use harness_memory::selection::{SelectionParams, select_for_turn};
use harness_proto::ErrorCode;
use harness_rpc::{Handler, Sink, handler, parse_params};
use harness_session::{
    AgentRecord, SessionEvent,
    agent::{self, AgentStatus},
    message::{self, MessageRole},
};
use harness_tools::{Registry, ToolContext};
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;

#[derive(Deserialize)]
struct ChatSendParams {
    session_id: String,
    message: String,
    #[serde(default)]
    model: Option<String>,
}

pub fn send(d: Arc<Daemon>) -> Arc<dyn Handler> {
    handler(move |p, sink: Sink| {
        let d = d.clone();
        async move {
            let params: ChatSendParams = parse_params(p)?;
            execute_chat_turn(&d, params, sink).await
        }
    })
}

pub fn cancel(d: Arc<Daemon>) -> Arc<dyn Handler> {
    handler(move |p, _s| {
        let d = d.clone();
        async move {
            #[derive(Deserialize)]
            struct Params {
                agent_id: String,
            }
            let Params { agent_id } = parse_params(p)?;
            let aid =
                AgentId::parse(&agent_id).map_err(|e| rpc_err(ErrorCode::InvalidParams, e))?;
            agent::set_status(&d.storage.writer, aid, AgentStatus::Failed)
                .await
                .map_err(|e| rpc_err(ErrorCode::InternalError, e))?;
            Ok(json!({"cancelled": true}))
        }
    })
}

async fn execute_chat_turn(
    d: &Arc<Daemon>,
    params: ChatSendParams,
    sink: Sink,
) -> Result<Value, harness_proto::ErrorObject> {
    let ChatSendParams {
        session_id,
        message,
        model,
    } = params;
    let sid = SessionId::parse(&session_id).map_err(|e| rpc_err(ErrorCode::InvalidParams, e))?;

    let reader = d
        .storage
        .readers
        .get()
        .map_err(|e| rpc_err(ErrorCode::InternalError, e))?;
    let session =
        harness_session::manager::SessionManager::get(&reader, sid).map_err(map_session_err)?;
    let agents =
        agent::list_for_session(&reader, sid).map_err(|e| rpc_err(ErrorCode::InternalError, e))?;
    let root = agents
        .iter()
        .find(|a| a.parent_id.is_none())
        .ok_or_else(|| rpc_err(ErrorCode::NotFound, "session has no root agent"))?;

    let model_id = model.unwrap_or_else(|| root.model.clone());
    let model_meta = d.llm.models.read().await.get(&model_id).cloned();
    let Some(model_meta) = model_meta else {
        return Err(rpc_err(
            ErrorCode::InvalidParams,
            format!("model {model_id} is not registered"),
        ));
    };
    let family = model_meta.provider.clone();

    let prior_messages =
        load_prior_messages(&reader, &agents).map_err(|e| rpc_err(ErrorCode::InternalError, e))?;

    let memories = select_for_turn(
        &reader,
        &SelectionParams {
            cwd: &session.cwd,
            query: Some(&message),
            context_window: model_meta.context_window as usize,
            budget_bps: 1_000,
        },
    )
    .map_err(|e| rpc_err(ErrorCode::InternalError, e))?;
    drop(reader);

    let tool_defs = registry_to_tool_defs(&d.tools.registry);

    let role_label = root.role.clone();
    let task_text = session
        .task
        .clone()
        .unwrap_or_else(|| "(no task specified)".into());
    let skill_snapshot: Vec<(String, String, String)> = d
        .tools
        .skills
        .read()
        .map_err(|_| rpc_err(ErrorCode::InternalError, "skills lock poisoned"))?
        .iter()
        .map(|s| {
            (
                s.name.clone(),
                s.description.clone(),
                s.location.to_string_lossy().into_owned(),
            )
        })
        .collect();
    let skill_summaries: Vec<harness_context::SkillSummary<'_>> = skill_snapshot
        .iter()
        .map(|(n, desc, loc)| harness_context::SkillSummary {
            name: n.as_str(),
            description: desc.as_str(),
            location: loc.as_str(),
        })
        .collect();
    let assembled = assemble(&AssemblyInputs {
        role: &role_label,
        task: &task_text,
        tools: &tool_defs,
        memories: &memories,
        skills: &skill_summaries,
        messages: &prior_messages,
    });

    let pool = {
        let guard = d.llm.providers.read().await;
        guard.clone().ok_or_else(|| {
            rpc_err(
                ErrorCode::InternalError,
                "no providers configured; use auth.credentials.add",
            )
        })?
    };

    let tool_context = ToolContext {
        session: sid,
        agent: root.id,
        cwd: session.cwd.clone().into(),
        allowed_writes: None,
        is_root: true,
        approval: Some(Arc::new(DaemonApprovalRequester {
            broadcaster: d.storage.broadcaster.clone(),
            pending: d.tools.pending_approvals.clone(),
        })),
    };

    let mut rx = d.storage.broadcaster.subscribe(sid);
    let notify_sink = sink.clone();
    let notify_task = tokio::spawn(async move {
        while let Ok(ev) = rx.recv().await {
            let terminal = matches!(ev, SessionEvent::Error { .. });
            if let Some(notification) = event_to_notification(sid, &ev)
                && let Ok(text) = serde_json::to_string(&notification)
                && notify_sink.send_raw(text).await.is_err()
            {
                break;
            }
            if terminal {
                break;
            }
        }
    });

    let outcome = run_turn(TurnInputs {
        session_id: sid,
        agent_id: root.id,
        model: model_id.clone(),
        family,
        user_message: message,
        system_prompt: assembled.system,
        tool_defs,
        prior_messages,
        pool,
        tool_registry: d.tools.registry.clone(),
        tool_context,
        writer: &d.storage.writer,
        broadcaster: &d.storage.broadcaster,
        max_iterations: DEFAULT_MAX_ITERATIONS,
    })
    .await
    .map_err(|e| rpc_err(ErrorCode::InternalError, e))?;

    notify_task.abort();

    Ok(match outcome {
        TurnOutcome::Done { message_id } => json!({
            "status": "done",
            "message_id": message_id.as_uuid().to_string(),
            "model": model_id,
        }),
        TurnOutcome::Failed { reason } => json!({
            "status": "failed",
            "reason": reason,
        }),
    })
}

fn load_prior_messages(
    conn: &rusqlite::Connection,
    agents: &[AgentRecord],
) -> Result<Vec<LlmMessage>, harness_session::SessionError> {
    let mut out: Vec<(i64, LlmMessage)> = Vec::new();
    for a in agents {
        let rows = message::list_for_agent(conn, a.id)?;
        for m in rows {
            let Some(text) = m.content else { continue };
            if text.is_empty() {
                continue;
            }
            let role = match m.role {
                MessageRole::User => LlmRole::User,
                MessageRole::Assistant => LlmRole::Assistant,
                MessageRole::System => LlmRole::System,
            };
            out.push((
                m.created_at,
                LlmMessage {
                    role,
                    content: vec![ContentBlock::Text { text }],
                },
            ));
        }
    }
    out.sort_by_key(|(ts, _)| *ts);
    Ok(out.into_iter().map(|(_, m)| m).collect())
}

fn registry_to_tool_defs(registry: &Registry) -> Vec<ToolDef> {
    registry
        .names()
        .into_iter()
        .filter_map(|n| {
            registry.get(&n).map(|t| ToolDef {
                name: t.name().to_string(),
                description: t.description().to_string(),
                input_schema: t.input_schema(),
            })
        })
        .collect()
}
