use crate::{io_err, resolve};
use async_trait::async_trait;
use harness_tools::{Sandbox, SandboxPolicy, Tool, ToolContext, ToolError, ToolOutput};
use harness_tools_code::{EditError, HashlineAnchor, apply_hashline_edit, apply_string_replace};
use serde::Deserialize;
use serde_json::{Value, json};

pub struct EditTool;

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum EditInput {
    Hashline {
        path: String,
        anchors: Vec<HashlineAnchor>,
    },
    Exact {
        path: String,
        old_string: String,
        new_string: String,
        #[serde(default)]
        replace_all: Option<bool>,
    },
}

#[async_trait]
impl Tool for EditTool {
    fn name(&self) -> &'static str {
        "edit"
    }

    fn description(&self) -> &'static str {
        "Edit a file. Two input shapes are accepted:\n\
         1. Hashline (preferred): `{path, anchors: [{line: \"NN:HH\", \
         content: \"new line\"}]}` — anchors refer to lines as read via \
         the `read` tool.\n\
         2. Exact string: `{path, old_string, new_string, replace_all?}`.\n\n\
         USE: modify existing files.\n\
         DO NOT USE: creating new files (use `write`)."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["path"],
            "properties": {
                "path": { "type": "string" },
                "anchors": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "required": ["line", "content"],
                        "properties": {
                            "line": { "type": "string", "description": "NN:HH anchor from `read`." },
                            "content": { "type": "string" }
                        }
                    }
                },
                "old_string": { "type": "string" },
                "new_string": { "type": "string" },
                "replace_all": { "type": "boolean", "default": false }
            }
        })
    }

    async fn execute(&self, input: Value, ctx: &ToolContext) -> Result<ToolOutput, ToolError> {
        let args: EditInput =
            serde_json::from_value(input).map_err(|e| ToolError::Input(e.to_string()))?;

        let path_str = match &args {
            EditInput::Hashline { path, .. } | EditInput::Exact { path, .. } => path.clone(),
        };
        let path = resolve(&ctx.cwd, &path_str);

        if !ctx.can_write(&path) {
            return Err(ToolError::OutOfScope {
                path: path.display().to_string(),
            });
        }
        if let SandboxPolicy::Deny { reason } = Sandbox::evaluate_write_path(&path) {
            return Err(ToolError::Denied(reason));
        }

        let current = tokio::fs::read_to_string(&path)
            .await
            .map_err(|e| io_err(&path, &e))?;

        let new = match args {
            EditInput::Hashline { anchors, .. } => {
                apply_hashline_edit(&current, &anchors).map_err(|e| map_edit_err(&e))?
            }
            EditInput::Exact {
                old_string,
                new_string,
                replace_all,
                ..
            } => apply_string_replace(
                &current,
                &old_string,
                &new_string,
                replace_all.unwrap_or(false),
            )
            .map_err(|e| map_edit_err(&e))?,
        };

        tokio::fs::write(&path, &new)
            .await
            .map_err(|e| io_err(&path, &e))?;

        let diff_lines = diff_line_count(&current, &new);
        Ok(ToolOutput::ok(format!(
            "edited {} ({} lines changed)",
            path.display(),
            diff_lines
        )))
    }
}

fn map_edit_err(e: &EditError) -> ToolError {
    ToolError::Other(e.to_string())
}

fn diff_line_count(before: &str, after: &str) -> usize {
    let before_lines: Vec<&str> = before.lines().collect();
    let after_lines: Vec<&str> = after.lines().collect();
    let mut changed = 0usize;
    let m = before_lines.len().max(after_lines.len());
    for i in 0..m {
        let b = before_lines.get(i).copied().unwrap_or("");
        let a = after_lines.get(i).copied().unwrap_or("");
        if a != b {
            changed += 1;
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use harness_tools_code::hashline::hash_line;
    use tempfile::TempDir;

    #[tokio::test]
    async fn hashline_edit_succeeds() {
        let t = TempDir::new().unwrap();
        let path = t.path().join("a.txt");
        tokio::fs::write(&path, "alpha\nbeta\ngamma\n")
            .await
            .unwrap();
        let h2 = hash_line("beta");
        EditTool
            .execute(
                json!({
                    "path": "a.txt",
                    "anchors": [{"line": format!("2:{h2}"), "content": "BETA"}]
                }),
                &ToolContext::test(t.path()),
            )
            .await
            .unwrap();
        let after = tokio::fs::read_to_string(&path).await.unwrap();
        assert_eq!(after, "alpha\nBETA\ngamma\n");
    }

    #[tokio::test]
    async fn hashline_mismatch_returns_error() {
        let t = TempDir::new().unwrap();
        let path = t.path().join("a.txt");
        tokio::fs::write(&path, "alpha\nbeta\n").await.unwrap();
        let err = EditTool
            .execute(
                json!({
                    "path": "a.txt",
                    "anchors": [{"line": "2:zz", "content": "B"}]
                }),
                &ToolContext::test(t.path()),
            )
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::Other(_)));
        let after = tokio::fs::read_to_string(&path).await.unwrap();
        assert_eq!(after, "alpha\nbeta\n");
    }

    #[tokio::test]
    async fn exact_string_replace_works() {
        let t = TempDir::new().unwrap();
        let path = t.path().join("a.txt");
        tokio::fs::write(&path, "foo bar foo").await.unwrap();
        EditTool
            .execute(
                json!({
                    "path": "a.txt",
                    "old_string": "bar",
                    "new_string": "baz",
                }),
                &ToolContext::test(t.path()),
            )
            .await
            .unwrap();
        assert_eq!(
            tokio::fs::read_to_string(&path).await.unwrap(),
            "foo baz foo"
        );
    }

    #[tokio::test]
    async fn exact_string_rejects_ambiguous() {
        let t = TempDir::new().unwrap();
        let path = t.path().join("a.txt");
        tokio::fs::write(&path, "foo foo").await.unwrap();
        let err = EditTool
            .execute(
                json!({
                    "path": "a.txt",
                    "old_string": "foo",
                    "new_string": "bar",
                }),
                &ToolContext::test(t.path()),
            )
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::Other(_)));
    }

    #[tokio::test]
    async fn scope_enforcement() {
        let t = TempDir::new().unwrap();
        let path = t.path().join("locked.txt");
        tokio::fs::write(&path, "x").await.unwrap();
        let ctx = ToolContext {
            session: harness_core::SessionId::new(),
            agent: harness_core::AgentId::new(),
            cwd: t.path().to_path_buf(),
            allowed_writes: Some(vec!["elsewhere.txt".into()]),
            is_root: false,
            approval: None,
        };
        let err = EditTool
            .execute(
                json!({
                    "path": "locked.txt",
                    "old_string": "x",
                    "new_string": "y",
                }),
                &ctx,
            )
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::OutOfScope { .. }));
    }
}
