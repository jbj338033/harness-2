use crate::client::{LspError, path_to_uri};
use crate::pool::LspPool;
use async_trait::async_trait;
use harness_tools::{Tool, ToolContext, ToolError, ToolOutput};
use serde::Deserialize;
use serde_json::{Value, json};
use std::fmt::Write;
use std::fs;
use std::path::PathBuf;

pub struct LspTool {
    pool: LspPool,
}

impl LspTool {
    #[must_use]
    pub fn new(pool: LspPool) -> Self {
        Self { pool }
    }
}

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
enum Input {
    Definition {
        language: String,
        path: String,
        line: u32,
        character: u32,
    },
    References {
        language: String,
        path: String,
        line: u32,
        character: u32,
    },
    Rename {
        language: String,
        path: String,
        line: u32,
        character: u32,
        new_name: String,
    },
    Diagnostics {
        language: String,
        path: String,
    },
}

#[async_trait]
impl Tool for LspTool {
    fn name(&self) -> &'static str {
        "lsp"
    }
    fn description(&self) -> &'static str {
        "Project-aware code navigation via Language Server Protocol.\n\
         USE: go-to-definition, find-references, rename, diagnostics.\n\
         DO NOT USE: text search (use grep) or file parsing (use read outline)."
    }
    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["action", "language"],
            "oneOf": [
                {"required": ["action","language","path","line","character"],
                 "properties": {"action": {"const": "definition"}}},
                {"required": ["action","language","path","line","character"],
                 "properties": {"action": {"const": "references"}}},
                {"required": ["action","language","path","line","character","new_name"],
                 "properties": {"action": {"const": "rename"}}},
                {"required": ["action","language","path"],
                 "properties": {"action": {"const": "diagnostics"}}}
            ]
        })
    }

    async fn execute(&self, input: Value, ctx: &ToolContext) -> Result<ToolOutput, ToolError> {
        let parsed: Input =
            serde_json::from_value(input).map_err(|e| ToolError::Input(e.to_string()))?;

        match parsed {
            Input::Definition {
                language,
                path,
                line,
                character,
            } => {
                let (client, uri) = prepare(&self.pool, ctx, &language, &path).await?;
                let result = client
                    .request(
                        "textDocument/definition",
                        json!({
                            "textDocument": {"uri": uri},
                            "position": {"line": line, "character": character},
                        }),
                    )
                    .await
                    .map_err(lsp_to_tool)?;
                Ok(ToolOutput::ok(render_locations(&result)).with_metadata(result))
            }
            Input::References {
                language,
                path,
                line,
                character,
            } => {
                let (client, uri) = prepare(&self.pool, ctx, &language, &path).await?;
                let result = client
                    .request(
                        "textDocument/references",
                        json!({
                            "textDocument": {"uri": uri},
                            "position": {"line": line, "character": character},
                            "context": {"includeDeclaration": true},
                        }),
                    )
                    .await
                    .map_err(lsp_to_tool)?;
                Ok(ToolOutput::ok(render_locations(&result)).with_metadata(result))
            }
            Input::Rename {
                language,
                path,
                line,
                character,
                new_name,
            } => {
                let (client, uri) = prepare(&self.pool, ctx, &language, &path).await?;
                let result = client
                    .request(
                        "textDocument/rename",
                        json!({
                            "textDocument": {"uri": uri},
                            "position": {"line": line, "character": character},
                            "newName": new_name,
                        }),
                    )
                    .await
                    .map_err(lsp_to_tool)?;
                let body = serde_json::to_string_pretty(&result).unwrap_or_default();
                Ok(ToolOutput::ok(body).with_metadata(result))
            }
            Input::Diagnostics { language, path } => {
                let (client, uri) = prepare(&self.pool, ctx, &language, &path).await?;
                let diags = client.diagnostics(&uri).await;
                let rendered = render_diagnostics(&diags);
                Ok(ToolOutput::ok(rendered).with_metadata(json!({"diagnostics": diags})))
            }
        }
    }
}

async fn prepare(
    pool: &LspPool,
    ctx: &ToolContext,
    language: &str,
    path: &str,
) -> Result<(std::sync::Arc<crate::LspClient>, String), ToolError> {
    let client = pool
        .acquire(ctx.cwd.clone(), language)
        .await
        .map_err(lsp_to_tool)?;
    let abs: PathBuf = if std::path::Path::new(path).is_absolute() {
        PathBuf::from(path)
    } else {
        ctx.cwd.join(path)
    };
    let uri = path_to_uri(&abs);
    let body = fs::read_to_string(&abs).map_err(ToolError::Io)?;
    client
        .notify(
            "textDocument/didOpen",
            json!({
                "textDocument": {
                    "uri": uri,
                    "languageId": language,
                    "version": 1,
                    "text": body,
                }
            }),
        )
        .await
        .map_err(lsp_to_tool)?;
    Ok((client, uri))
}

fn render_locations(v: &Value) -> String {
    let arr: Vec<&Value> = match v {
        Value::Array(a) => a.iter().collect(),
        Value::Null => return "no locations".into(),
        other => vec![other],
    };
    let mut out = String::new();
    for loc in arr {
        let uri = loc.get("uri").and_then(Value::as_str).unwrap_or("?");
        let line = loc
            .pointer("/range/start/line")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let ch = loc
            .pointer("/range/start/character")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        writeln!(out, "{uri}:{line}:{ch}").unwrap();
    }
    out
}

fn render_diagnostics(diags: &[Value]) -> String {
    if diags.is_empty() {
        return "no diagnostics".into();
    }
    let mut out = String::new();
    for d in diags {
        let line = d
            .pointer("/range/start/line")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let ch = d
            .pointer("/range/start/character")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let severity = d.get("severity").and_then(Value::as_u64).unwrap_or(0);
        let msg = d.get("message").and_then(Value::as_str).unwrap_or("");
        writeln!(out, "{line}:{ch} [sev={severity}] {msg}").unwrap();
    }
    out
}

fn lsp_to_tool(e: LspError) -> ToolError {
    match e {
        LspError::Io(io) => ToolError::Io(io),
        other => ToolError::Other(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_has_oneof() {
        let t = LspTool::new(LspPool::new());
        assert!(t.input_schema()["oneOf"].is_array());
    }

    #[tokio::test]
    async fn unknown_language_rejected() {
        let t = LspTool::new(LspPool::new());
        let ctx = ToolContext::test(std::env::temp_dir());
        let err = t
            .execute(
                json!({
                    "action": "diagnostics",
                    "language": "lean",
                    "path": "x.lean"
                }),
                &ctx,
            )
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::Other(_)));
    }

    #[test]
    fn render_locations_formats_uri_range() {
        let v = json!([{
            "uri": "file:///a/b.rs",
            "range": {"start": {"line": 3, "character": 5}, "end": {"line": 3, "character": 10}}
        }]);
        let text = render_locations(&v);
        assert!(text.contains("file:///a/b.rs:3:5"));
    }

    #[test]
    fn render_diagnostics_empty_message() {
        let text = render_diagnostics(&[]);
        assert_eq!(text, "no diagnostics");
    }

    #[test]
    fn render_diagnostics_includes_severity() {
        let d = json!({
            "range": {"start": {"line": 1, "character": 0}, "end": {"line": 1, "character": 1}},
            "severity": 1,
            "message": "error here",
        });
        let text = render_diagnostics(&[d]);
        assert!(text.contains("error here"));
    }
}
