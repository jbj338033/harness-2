use crate::AgentTools;
use async_trait::async_trait;
use harness_core::AgentId;
use harness_session::{
    agent::{self, AgentStatus},
    broadcast::SessionEvent,
};
use harness_tools::{Tool, ToolContext, ToolError, ToolOutput};
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;
use tracing::info;

pub struct CancelTool(pub Arc<AgentTools>);

#[derive(Deserialize)]
struct Input {
    agent_id: String,
    #[serde(default)]
    reason: Option<String>,
}

#[async_trait]
impl Tool for CancelTool {
    fn name(&self) -> &'static str {
        "cancel"
    }

    fn description(&self) -> &'static str {
        "Mark a subagent as failed (cooperative cancel).\n\
         USE: when you've decided a running subagent is no longer needed.\n\
         DO NOT USE: to stop your OWN turn — return control instead."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["agent_id"],
            "properties": {
                "agent_id": {"type": "string", "description": "id from spawn"},
                "reason": {"type": "string", "description": "optional note"}
            }
        })
    }

    async fn execute(&self, input: Value, ctx: &ToolContext) -> Result<ToolOutput, ToolError> {
        let Input { agent_id, reason } =
            serde_json::from_value(input).map_err(|e| ToolError::Input(e.to_string()))?;
        let target = AgentId::parse(&agent_id).map_err(|e| ToolError::Input(e.to_string()))?;

        agent::set_status(&self.0.writer, target, AgentStatus::Failed)
            .await
            .map_err(|e| ToolError::Other(e.to_string()))?;

        self.0.broadcaster.publish(
            ctx.session,
            SessionEvent::AgentStatus {
                agent_id: target,
                status: "failed".into(),
            },
        );

        let msg = reason.map_or_else(
            || format!("cancelled {target}"),
            |r| format!("cancelled {target}: {r}"),
        );
        info!(parent = %ctx.agent, target = %target, "cancelled subagent");
        Ok(ToolOutput::ok(msg)
            .with_metadata(json!({"agent_id": target.to_string(), "status": "failed"})))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use harness_session::{SessionBroadcaster, agent::NewAgent, manager::SessionManager};
    use harness_storage::{Database, Writer};
    use tempfile::NamedTempFile;

    async fn setup() -> (NamedTempFile, Arc<AgentTools>, ToolContext, AgentId) {
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
            is_root: true,
            approval: None,
        };
        (f, tools, ctx, child)
    }

    #[tokio::test]
    async fn cancel_marks_agent_failed() {
        let (f, tools, ctx, child) = setup().await;
        let tool = CancelTool(tools);
        let out = tool
            .execute(json!({"agent_id": child.to_string()}), &ctx)
            .await
            .unwrap();
        assert!(out.content.contains("cancelled"));

        let reader = rusqlite::Connection::open(f.path()).unwrap();
        let rec = harness_session::agent::get(&reader, child).unwrap();
        assert_eq!(rec.status, harness_session::AgentStatus::Failed);
    }

    #[tokio::test]
    async fn cancel_rejects_bad_id() {
        let (_f, tools, ctx, _child) = setup().await;
        let tool = CancelTool(tools);
        let err = tool
            .execute(json!({"agent_id": "not-a-uuid"}), &ctx)
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::Input(_)));
    }
}
