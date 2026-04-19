use crate::AgentTools;
use async_trait::async_trait;
use harness_core::AgentId;
use harness_session::agent;
use harness_tools::{Tool, ToolContext, ToolError, ToolOutput};
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::{Instant, sleep};

pub struct WaitTool(pub Arc<AgentTools>);

#[derive(Deserialize)]
struct Input {
    agent_ids: Vec<String>,
    #[serde(default = "default_timeout_secs")]
    timeout_secs: u64,
}

fn default_timeout_secs() -> u64 {
    1800
}

#[async_trait]
impl Tool for WaitTool {
    fn name(&self) -> &'static str {
        "wait"
    }

    fn description(&self) -> &'static str {
        "Block until all given agents reach done/failed.\n\
         USE: after spawning workers and before acting on their output.\n\
         DO NOT USE: for time-based sleeps — this is not a sleep primitive."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["agent_ids"],
            "properties": {
                "agent_ids": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "agent ids returned by spawn"
                },
                "timeout_secs": {
                    "type": "integer",
                    "description": "upper bound on wait time (default 1800)"
                }
            }
        })
    }

    async fn execute(&self, input: Value, _ctx: &ToolContext) -> Result<ToolOutput, ToolError> {
        let Input {
            agent_ids,
            timeout_secs,
        } = serde_json::from_value(input).map_err(|e| ToolError::Input(e.to_string()))?;

        let parsed: Vec<AgentId> = agent_ids
            .into_iter()
            .map(|s| AgentId::parse(&s).map_err(|e| ToolError::Input(e.to_string())))
            .collect::<Result<_, _>>()?;

        let deadline = Instant::now() + Duration::from_secs(timeout_secs);
        let statuses: Vec<(AgentId, harness_session::AgentStatus)>;

        loop {
            let db = self.0.db_path.clone();
            let ids = parsed.clone();
            let current: Vec<(AgentId, harness_session::AgentStatus)> =
                tokio::task::spawn_blocking(move || -> Result<_, ToolError> {
                    let conn = rusqlite::Connection::open(&db)
                        .map_err(|e| ToolError::Other(e.to_string()))?;
                    let mut out = Vec::with_capacity(ids.len());
                    for id in ids {
                        let rec =
                            agent::get(&conn, id).map_err(|e| ToolError::Other(e.to_string()))?;
                        out.push((id, rec.status));
                    }
                    Ok(out)
                })
                .await
                .map_err(|e| ToolError::Other(e.to_string()))??;

            let all_terminal = current.iter().all(|(_, s)| s.is_terminal());
            if all_terminal {
                statuses = current;
                break;
            }
            if Instant::now() >= deadline {
                return Ok(ToolOutput::err(format!(
                    "wait timed out after {timeout_secs}s; still-running agents: {}",
                    current
                        .iter()
                        .filter(|(_, s)| !s.is_terminal())
                        .map(|(id, _)| id.to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                )));
            }
            sleep(Duration::from_millis(250)).await;
        }

        let summary: Vec<String> = statuses
            .iter()
            .map(|(id, s)| format!("{id}: {}", s.as_str()))
            .collect();
        let all_done = statuses
            .iter()
            .all(|(_, s)| matches!(s, harness_session::AgentStatus::Done));
        let body = summary.join("\n");
        Ok(if all_done {
            ToolOutput::ok(body)
        } else {
            ToolOutput::err(body)
        }
        .with_metadata(json!({
            "statuses": statuses.iter().map(|(id, s)| json!({"id": id.to_string(), "status": s.as_str()})).collect::<Vec<_>>()
        })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use harness_session::{
        AgentStatus, SessionBroadcaster, agent::NewAgent, manager::SessionManager,
    };
    use harness_storage::{Database, Writer, WriterHandle};
    use tempfile::NamedTempFile;

    async fn setup() -> (
        NamedTempFile,
        Arc<AgentTools>,
        WriterHandle,
        ToolContext,
        AgentId,
    ) {
        let f = NamedTempFile::new().unwrap();
        Database::open(f.path()).unwrap();
        let writer = Writer::spawn(f.path()).unwrap();
        let sm = SessionManager::new(&writer);
        let s = sm.create("/tmp", None).await.unwrap();
        let root = agent::insert(
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
        let child = agent::insert(
            &writer,
            NewAgent {
                session_id: s.id,
                parent_id: Some(root),
                role: "coder".into(),
                model: "mock".into(),
                system_prompt: None,
                worktree_path: None,
                wave: None,
            },
        )
        .await
        .unwrap();
        let tools = Arc::new(AgentTools {
            writer: writer.clone(),
            broadcaster: Arc::new(SessionBroadcaster::default()),
            db_path: f.path().to_path_buf(),
            default_model: "mock".into(),
        });
        let ctx = ToolContext {
            session: s.id,
            agent: root,
            cwd: "/tmp".into(),
            allowed_writes: None,
            is_root: true,
            approval: None,
        };
        (f, tools, writer, ctx, child)
    }

    #[tokio::test]
    async fn wait_returns_when_agent_done() {
        let (_f, tools, writer, ctx, child) = setup().await;
        agent::set_status(&writer, child, AgentStatus::Done)
            .await
            .unwrap();
        let tool = WaitTool(tools);
        let out = tool
            .execute(
                json!({"agent_ids": [child.to_string()], "timeout_secs": 2}),
                &ctx,
            )
            .await
            .unwrap();
        assert!(!out.is_error);
        assert!(out.content.contains("done"));
    }

    #[tokio::test]
    async fn wait_returns_failed_when_agent_failed() {
        let (_f, tools, writer, ctx, child) = setup().await;
        agent::set_status(&writer, child, AgentStatus::Failed)
            .await
            .unwrap();
        let tool = WaitTool(tools);
        let out = tool
            .execute(
                json!({"agent_ids": [child.to_string()], "timeout_secs": 2}),
                &ctx,
            )
            .await
            .unwrap();
        assert!(out.is_error);
        assert!(out.content.contains("failed"));
    }

    #[tokio::test]
    async fn wait_times_out_when_agent_still_running() {
        let (_f, tools, writer, ctx, child) = setup().await;
        agent::set_status(&writer, child, AgentStatus::Running)
            .await
            .unwrap();
        let tool = WaitTool(tools);
        let out = tool
            .execute(
                json!({"agent_ids": [child.to_string()], "timeout_secs": 1}),
                &ctx,
            )
            .await
            .unwrap();
        assert!(out.is_error);
        assert!(out.content.contains("timed out"));
    }
}
