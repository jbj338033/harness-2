use crate::Daemon;
use crate::rpc::errors::{map_session_err, rpc_err};
use crate::rpc::mappers::{agent_to_json, message_to_json, session_to_json};
use harness_core::SessionId;
use harness_proto::ErrorCode;
use harness_rpc::{Handler, handler, parse_params};
use harness_session::{
    agent::{self, NewAgent},
    manager::SessionManager,
};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;

pub const SESSION_LIST_LIMIT: usize = 50;

pub fn create(d: Arc<Daemon>) -> Arc<dyn Handler> {
    handler(move |p, _s| {
        let d = d.clone();
        async move {
            #[derive(Deserialize)]
            struct Params {
                cwd: String,
                #[serde(default)]
                task: Option<String>,
                #[serde(default)]
                model: Option<String>,
            }
            let Params { cwd, task, model } = parse_params(p)?;
            let sm = SessionManager::new(&d.storage.writer);
            let s = sm
                .create(cwd, task)
                .await
                .map_err(|e| rpc_err(ErrorCode::InternalError, e))?;
            let model = match model {
                Some(m) => m,
                None => d
                    .llm
                    .default_model
                    .read()
                    .map_err(|_| rpc_err(ErrorCode::InternalError, "default_model lock poisoned"))?
                    .clone(),
            };
            if model.is_empty() {
                return Err(rpc_err(
                    ErrorCode::InvalidParams,
                    "no default model — sign in first with `harness auth login`",
                ));
            }
            let root = agent::insert(
                &d.storage.writer,
                NewAgent {
                    session_id: s.id,
                    parent_id: None,
                    role: "root".into(),
                    model: model.clone(),
                    system_prompt: None,
                    worktree_path: None,
                    wave: None,
                },
            )
            .await
            .map_err(|e| rpc_err(ErrorCode::InternalError, e))?;
            Ok(json!({
                "session_id": s.id.as_uuid().to_string(),
                "agent_id": root.as_uuid().to_string(),
                "model": model,
                "cwd": s.cwd,
            }))
        }
    })
}

pub fn list(d: Arc<Daemon>) -> Arc<dyn Handler> {
    handler(move |_p, _s| {
        let d = d.clone();
        async move {
            let reader = d
                .storage
                .readers
                .get()
                .map_err(|e| rpc_err(ErrorCode::InternalError, e))?;
            let sessions = SessionManager::list(&reader, SESSION_LIST_LIMIT)
                .map_err(|e| rpc_err(ErrorCode::InternalError, e))?;
            Ok(json!({
                "sessions": sessions.iter().map(session_to_json).collect::<Vec<_>>()
            }))
        }
    })
}

pub fn resume(d: Arc<Daemon>) -> Arc<dyn Handler> {
    handler(move |p, _s| {
        let d = d.clone();
        async move {
            #[derive(Deserialize)]
            struct Params {
                session_id: String,
            }
            let Params { session_id } = parse_params(p)?;
            let sid =
                SessionId::parse(&session_id).map_err(|e| rpc_err(ErrorCode::InvalidParams, e))?;
            let reader = d
                .storage
                .readers
                .get()
                .map_err(|e| rpc_err(ErrorCode::InternalError, e))?;
            let s = SessionManager::get(&reader, sid).map_err(map_session_err)?;
            let agents = agent::list_for_session(&reader, sid)
                .map_err(|e| rpc_err(ErrorCode::InternalError, e))?;
            let messages = agents
                .iter()
                .flat_map(|a| {
                    harness_session::message::list_for_agent(&reader, a.id)
                        .unwrap_or_default()
                        .into_iter()
                        .map(|m| message_to_json(&m))
                })
                .collect::<Vec<_>>();
            Ok(json!({
                "session": session_to_json(&s),
                "agents": agents.iter().map(agent_to_json).collect::<Vec<_>>(),
                "messages": messages,
            }))
        }
    })
}

pub fn delete(d: Arc<Daemon>) -> Arc<dyn Handler> {
    handler(move |p, _s| {
        let d = d.clone();
        async move {
            #[derive(Deserialize)]
            struct Params {
                session_id: String,
            }
            let Params { session_id } = parse_params(p)?;
            let sid =
                SessionId::parse(&session_id).map_err(|e| rpc_err(ErrorCode::InvalidParams, e))?;
            let sm = SessionManager::new(&d.storage.writer);
            sm.delete(sid)
                .await
                .map_err(|e| rpc_err(ErrorCode::InternalError, e))?;
            Ok(json!({"deleted": true}))
        }
    })
}
