use crate::{io_err, resolve};
use async_trait::async_trait;
use harness_tools::{Sandbox, SandboxPolicy, Tool, ToolContext, ToolError, ToolOutput};
use serde::Deserialize;
use serde_json::{Value, json};

pub struct WriteTool;

#[derive(Debug, Deserialize)]
struct WriteInput {
    path: String,
    content: String,
    #[serde(default)]
    overwrite: Option<bool>,
}

#[async_trait]
impl Tool for WriteTool {
    fn name(&self) -> &'static str {
        "write"
    }

    fn description(&self) -> &'static str {
        "Create a file with the given content, or overwrite an existing \
         file (overwrite=true required).\n\
         USE: creating new files.\n\
         DO NOT USE: modifying existing files (use `edit` instead)."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["path", "content"],
            "properties": {
                "path": { "type": "string" },
                "content": { "type": "string" },
                "overwrite": { "type": "boolean", "default": false }
            }
        })
    }

    async fn execute(&self, input: Value, ctx: &ToolContext) -> Result<ToolOutput, ToolError> {
        let args: WriteInput =
            serde_json::from_value(input).map_err(|e| ToolError::Input(e.to_string()))?;
        let path = resolve(&ctx.cwd, &args.path);

        if !ctx.can_write(&path) {
            return Err(ToolError::OutOfScope {
                path: path.display().to_string(),
            });
        }
        match Sandbox::evaluate_write_path(&path) {
            SandboxPolicy::Deny { reason } => return Err(ToolError::Denied(reason)),
            SandboxPolicy::Confirm { .. } | SandboxPolicy::Allow => {}
        }

        let exists = tokio::fs::metadata(&path).await.is_ok();
        if exists && !args.overwrite.unwrap_or(false) {
            return Err(ToolError::Denied(format!(
                "{} already exists; use overwrite=true to replace",
                path.display()
            )));
        }

        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| io_err(parent, &e))?;
        }
        tokio::fs::write(&path, &args.content)
            .await
            .map_err(|e| io_err(&path, &e))?;

        Ok(
            ToolOutput::ok(format!("wrote {} bytes", args.content.len())).with_metadata(json!({
                "path": path.display().to_string(),
            })),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn creates_new_file() {
        let t = TempDir::new().unwrap();
        WriteTool
            .execute(
                json!({"path": "new.txt", "content": "hello"}),
                &ToolContext::test(t.path()),
            )
            .await
            .unwrap();
        assert_eq!(
            tokio::fs::read_to_string(t.path().join("new.txt"))
                .await
                .unwrap(),
            "hello"
        );
    }

    #[tokio::test]
    async fn refuses_overwrite_without_flag() {
        let t = TempDir::new().unwrap();
        tokio::fs::write(t.path().join("existing"), "old")
            .await
            .unwrap();
        let err = WriteTool
            .execute(
                json!({"path": "existing", "content": "new"}),
                &ToolContext::test(t.path()),
            )
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::Denied(_)));
    }

    #[tokio::test]
    async fn overwrites_with_flag() {
        let t = TempDir::new().unwrap();
        tokio::fs::write(t.path().join("existing"), "old")
            .await
            .unwrap();
        WriteTool
            .execute(
                json!({"path": "existing", "content": "new", "overwrite": true}),
                &ToolContext::test(t.path()),
            )
            .await
            .unwrap();
        assert_eq!(
            tokio::fs::read_to_string(t.path().join("existing"))
                .await
                .unwrap(),
            "new"
        );
    }

    #[tokio::test]
    async fn respects_write_scope() {
        let t = TempDir::new().unwrap();
        let ctx = ToolContext {
            session: harness_core::SessionId::new(),
            agent: harness_core::AgentId::new(),
            cwd: t.path().to_path_buf(),
            allowed_writes: Some(vec!["only.txt".into()]),
            is_root: false,
            approval: None,
        };
        let err = WriteTool
            .execute(json!({"path": "other.txt", "content": "x"}), &ctx)
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::OutOfScope { .. }));
    }

    #[tokio::test]
    async fn denies_sandboxed_path() {
        let t = TempDir::new().unwrap();
        let err = WriteTool
            .execute(
                json!({"path": "/etc/danger", "content": "x"}),
                &ToolContext::test(t.path()),
            )
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::Denied(_)));
    }
}
