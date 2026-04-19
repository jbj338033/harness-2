use crate::AgentTools;
use async_trait::async_trait;
use harness_core::AgentId;
use harness_session::{
    agent::{self, NewAgent},
    broadcast::SessionEvent,
};
use harness_tools::{Tool, ToolContext, ToolError, ToolOutput};
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;
use tracing::info;

pub struct SpawnTool(pub Arc<AgentTools>);

#[derive(Deserialize)]
struct Input {
    role: String,
    task: String,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    worktree: Option<String>,
    #[serde(default)]
    system_prompt: Option<String>,
    #[serde(default)]
    wave: Option<i64>,
}

#[async_trait]
impl Tool for SpawnTool {
    fn name(&self) -> &'static str {
        "spawn"
    }

    fn description(&self) -> &'static str {
        "Create a subagent in the current session.\n\
         USE: when a task is complex enough to delegate part of it to a worker with fresh context.\n\
         DO NOT USE: for simple file edits; handle those inline with `edit`/`write`."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["role", "task"],
            "properties": {
                "role": {"type": "string", "description": "role label: coder / reviewer / tester / …"},
                "task": {"type": "string", "description": "task description; becomes the worker's initial user message"},
                "model": {"type": "string", "description": "optional model override"},
                "worktree": {"type": "string", "description": "optional git worktree path"},
                "system_prompt": {"type": "string", "description": "optional role prompt"},
                "wave": {"type": "integer", "description": "optional parallel wave number"}
            }
        })
    }

    async fn execute(&self, input: Value, ctx: &ToolContext) -> Result<ToolOutput, ToolError> {
        let Input {
            role,
            task,
            model,
            worktree,
            system_prompt,
            wave,
        } = serde_json::from_value(input).map_err(|e| ToolError::Input(e.to_string()))?;

        let new = NewAgent {
            session_id: ctx.session,
            parent_id: Some(ctx.agent),
            role: role.clone(),
            model: model.unwrap_or_else(|| self.0.default_model.clone()),
            system_prompt,
            worktree_path: worktree,
            wave,
        };

        let new_id: AgentId = agent::insert(&self.0.writer, new)
            .await
            .map_err(|e| ToolError::Other(e.to_string()))?;

        self.0.broadcaster.publish(
            ctx.session,
            SessionEvent::AgentStatus {
                agent_id: new_id,
                status: "pending".into(),
            },
        );

        info!(parent = %ctx.agent, child = %new_id, %role, "spawned subagent");
        Ok(ToolOutput::ok(format!(
            "spawned agent {new_id} (role: {role})\ntask: {task}"
        ))
        .with_metadata(json!({"agent_id": new_id.to_string(), "role": role})))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use harness_session::{SessionBroadcaster, manager::SessionManager};
    use harness_storage::{Database, Writer};
    use tempfile::NamedTempFile;

    async fn setup() -> (NamedTempFile, Arc<AgentTools>, ToolContext) {
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
        let tools = Arc::new(AgentTools {
            writer,
            broadcaster: Arc::new(SessionBroadcaster::default()),
            db_path: f.path().to_path_buf(),
            default_model: "mock".into(),
        });
        let ctx = ToolContext {
            session: s.id,
            agent: root,
            cwd: "/tmp".into(),
            allowed_writes: None,
            approval: None,
            is_root: true,
        };
        (f, tools, ctx)
    }

    #[tokio::test]
    async fn spawn_inserts_child_agent() {
        let (f, tools, ctx) = setup().await;
        let tool = SpawnTool(tools);
        let input = json!({"role": "coder", "task": "refactor"});
        let out = tool.execute(input, &ctx).await.unwrap();
        assert!(out.content.contains("spawned agent"));

        let reader = rusqlite::Connection::open(f.path()).unwrap();
        let agents = harness_session::agent::list_for_session(&reader, ctx.session).unwrap();
        assert_eq!(agents.len(), 2);
        let child = agents.iter().find(|a| a.role == "coder").unwrap();
        assert_eq!(child.status, harness_session::AgentStatus::Pending);
    }

    #[tokio::test]
    async fn spawn_missing_required_field() {
        let (_f, tools, ctx) = setup().await;
        let tool = SpawnTool(tools);
        let err = tool
            .execute(json!({"role": "coder"}), &ctx)
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::Input(_)));
    }
}
