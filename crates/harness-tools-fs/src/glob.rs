use crate::resolve;
use async_trait::async_trait;
use harness_tools::{Tool, ToolContext, ToolError, ToolOutput};
use ignore::WalkBuilder;
use serde::Deserialize;
use serde_json::{Value, json};

pub struct GlobTool;

#[derive(Debug, Deserialize)]
struct GlobInput {
    pattern: String,
    #[serde(default)]
    cwd: Option<String>,
    #[serde(default)]
    limit: Option<usize>,
}

#[async_trait]
impl Tool for GlobTool {
    fn name(&self) -> &'static str {
        "glob"
    }

    fn description(&self) -> &'static str {
        "Find files matching a glob pattern. Respects .gitignore by default.\n\
         Pattern examples: \"**/*.rs\", \"src/**/*.ts\", \"Cargo.toml\".\n\
         USE: locating files by name/extension.\n\
         DO NOT USE: searching file content (use `grep`)."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["pattern"],
            "properties": {
                "pattern": { "type": "string" },
                "cwd": { "type": "string" },
                "limit": { "type": "integer", "minimum": 1 }
            }
        })
    }

    async fn execute(&self, input: Value, ctx: &ToolContext) -> Result<ToolOutput, ToolError> {
        let args: GlobInput =
            serde_json::from_value(input).map_err(|e| ToolError::Input(e.to_string()))?;

        let root = args
            .cwd
            .as_deref()
            .map_or_else(|| ctx.cwd.clone(), |c| resolve(&ctx.cwd, c));
        let pattern = args.pattern.clone();
        let limit = args.limit.unwrap_or(1000);

        let matches = tokio::task::spawn_blocking(move || -> Result<Vec<String>, String> {
            let glob = globset::Glob::new(&pattern)
                .map_err(|e| e.to_string())?
                .compile_matcher();
            let mut out = Vec::new();
            let walker = WalkBuilder::new(&root).hidden(false).build();
            for entry in walker.flatten() {
                if out.len() >= limit {
                    break;
                }
                let path = entry.path();
                let Ok(rel) = path.strip_prefix(&root) else {
                    continue;
                };
                if rel.as_os_str().is_empty() {
                    continue;
                }
                if glob.is_match(rel) {
                    out.push(rel.display().to_string());
                }
            }
            out.sort();
            Ok(out)
        })
        .await
        .map_err(|e| ToolError::Other(e.to_string()))?
        .map_err(ToolError::Input)?;

        let content = if matches.is_empty() {
            "(no matches)".to_string()
        } else {
            matches.join("\n")
        };
        Ok(ToolOutput::ok(content).with_metadata(json!({
            "count": matches.len(),
        })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn finds_by_extension() {
        let t = TempDir::new().unwrap();
        tokio::fs::write(t.path().join("a.rs"), "").await.unwrap();
        tokio::fs::write(t.path().join("b.rs"), "").await.unwrap();
        tokio::fs::write(t.path().join("c.txt"), "").await.unwrap();

        let out = GlobTool
            .execute(json!({"pattern": "*.rs"}), &ToolContext::test(t.path()))
            .await
            .unwrap();
        assert!(out.content.contains("a.rs"));
        assert!(out.content.contains("b.rs"));
        assert!(!out.content.contains("c.txt"));
    }

    #[tokio::test]
    async fn recursive_pattern() {
        let t = TempDir::new().unwrap();
        tokio::fs::create_dir_all(t.path().join("src/sub"))
            .await
            .unwrap();
        tokio::fs::write(t.path().join("src/lib.rs"), "")
            .await
            .unwrap();
        tokio::fs::write(t.path().join("src/sub/mod.rs"), "")
            .await
            .unwrap();

        let out = GlobTool
            .execute(
                json!({"pattern": "src/**/*.rs"}),
                &ToolContext::test(t.path()),
            )
            .await
            .unwrap();
        assert!(out.content.contains("lib.rs"));
        assert!(out.content.contains("mod.rs"));
    }

    #[tokio::test]
    async fn no_matches_is_not_an_error() {
        let t = TempDir::new().unwrap();
        let out = GlobTool
            .execute(json!({"pattern": "*.rs"}), &ToolContext::test(t.path()))
            .await
            .unwrap();
        assert!(out.content.contains("no matches"));
    }
}
