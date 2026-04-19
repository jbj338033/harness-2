use crate::resolve;
use async_trait::async_trait;
use harness_tools::{Tool, ToolContext, ToolError, ToolOutput};
use ignore::WalkBuilder;
use serde::Deserialize;
use serde_json::{Value, json};
use std::io::{BufRead, BufReader};

pub struct GrepTool;

#[derive(Debug, Deserialize)]
struct GrepInput {
    pattern: String,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    glob: Option<String>,
    #[serde(default)]
    case_sensitive: Option<bool>,
    #[serde(default)]
    limit: Option<usize>,
}

#[async_trait]
impl Tool for GrepTool {
    fn name(&self) -> &'static str {
        "grep"
    }

    fn description(&self) -> &'static str {
        "Search file contents using a regular expression. Respects .gitignore.\n\
         USE: finding where a symbol is used, text search in the codebase.\n\
         DO NOT USE: via `bash grep/rg`. Use this tool."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["pattern"],
            "properties": {
                "pattern": { "type": "string" },
                "path": { "type": "string", "description": "Directory to search. Defaults to cwd." },
                "glob": { "type": "string", "description": "Optional file-name glob filter." },
                "case_sensitive": { "type": "boolean", "default": true },
                "limit": { "type": "integer", "minimum": 1, "default": 500 }
            }
        })
    }

    async fn execute(&self, input: Value, ctx: &ToolContext) -> Result<ToolOutput, ToolError> {
        let args: GrepInput =
            serde_json::from_value(input).map_err(|e| ToolError::Input(e.to_string()))?;

        let root = args
            .path
            .as_deref()
            .map_or_else(|| ctx.cwd.clone(), |c| resolve(&ctx.cwd, c));
        let pattern = args.pattern.clone();
        let glob = args.glob.clone();
        let case_sensitive = args.case_sensitive.unwrap_or(true);
        let limit = args.limit.unwrap_or(500);

        let hits = tokio::task::spawn_blocking(move || -> Result<Vec<String>, String> {
            let re = {
                let mut builder = regex_lite::RegexBuilder::new(&pattern);
                builder.case_insensitive(!case_sensitive);
                builder.build().map_err(|e| e.to_string())?
            };
            let glob_matcher = glob
                .as_deref()
                .map(|g| globset::Glob::new(g).map(|g| g.compile_matcher()))
                .transpose()
                .map_err(|e| e.to_string())?;

            let mut out = Vec::new();
            let walker = WalkBuilder::new(&root).build();
            'outer: for entry in walker.flatten() {
                let path = entry.path();
                if !path.is_file() {
                    continue;
                }
                let Ok(rel) = path.strip_prefix(&root) else {
                    continue;
                };
                if let Some(g) = &glob_matcher
                    && !g.is_match(rel)
                {
                    continue;
                }
                let Ok(f) = std::fs::File::open(path) else {
                    continue;
                };
                let reader = BufReader::new(f);
                for (idx, line_result) in reader.lines().enumerate() {
                    let Ok(line) = line_result else { break };
                    if re.is_match(&line) {
                        out.push(format!("{}:{}:{}", rel.display(), idx + 1, line));
                        if out.len() >= limit {
                            break 'outer;
                        }
                    }
                }
            }
            Ok(out)
        })
        .await
        .map_err(|e| ToolError::Other(e.to_string()))?
        .map_err(ToolError::Input)?;

        let content = if hits.is_empty() {
            "(no matches)".to_string()
        } else {
            hits.join("\n")
        };
        Ok(ToolOutput::ok(content).with_metadata(json!({
            "count": hits.len(),
        })))
    }
}
