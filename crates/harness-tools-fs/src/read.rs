use crate::{io_err, resolve};
use async_trait::async_trait;
use harness_tools::{Tool, ToolContext, ToolError, ToolOutput};
use harness_tools_code::annotate;
use serde::Deserialize;
use serde_json::{Value, json};

pub struct ReadTool;

#[derive(Debug, Deserialize)]
struct ReadInput {
    path: String,
    #[serde(default)]
    start_line: Option<usize>,
    #[serde(default)]
    end_line: Option<usize>,
    #[serde(default)]
    hashlines: Option<bool>,
}

#[async_trait]
impl Tool for ReadTool {
    fn name(&self) -> &'static str {
        "read"
    }

    fn description(&self) -> &'static str {
        "Read a file. Returns content annotated with hashline anchors \
         (`NNN:HH|line`) by default; pass hashlines=false for a plain read.\n\
         USE: inspect existing source before editing.\n\
         DO NOT USE: for directory listings (use glob); for searching \
         content (use grep)."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["path"],
            "properties": {
                "path": { "type": "string", "description": "Path relative to cwd or absolute." },
                "start_line": { "type": "integer", "minimum": 1 },
                "end_line": { "type": "integer", "minimum": 1 },
                "hashlines": { "type": "boolean", "default": true }
            }
        })
    }

    async fn execute(&self, input: Value, ctx: &ToolContext) -> Result<ToolOutput, ToolError> {
        let args: ReadInput =
            serde_json::from_value(input).map_err(|e| ToolError::Input(e.to_string()))?;
        let path = resolve(&ctx.cwd, &args.path);

        let content = tokio::fs::read_to_string(&path)
            .await
            .map_err(|e| io_err(&path, &e))?;

        let use_hashlines = args.hashlines.unwrap_or(true);
        let body = if use_hashlines {
            let annotated = annotate(&content);
            slice_lines(&annotated, args.start_line, args.end_line)
        } else {
            slice_lines(&content, args.start_line, args.end_line)
        };

        Ok(ToolOutput::ok(body).with_metadata(json!({
            "path": path.display().to_string(),
            "hashlines": use_hashlines,
        })))
    }
}

fn slice_lines(text: &str, start: Option<usize>, end: Option<usize>) -> String {
    if start.is_none() && end.is_none() {
        return text.to_string();
    }
    let start = start.unwrap_or(1).saturating_sub(1);
    let end = end.unwrap_or(usize::MAX);
    text.lines()
        .enumerate()
        .filter(|(i, _)| *i >= start && *i < end)
        .map(|(_, l)| l)
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    async fn sample() -> (TempDir, std::path::PathBuf) {
        let t = TempDir::new().unwrap();
        let p = t.path().join("a.txt");
        tokio::fs::write(&p, "alpha\nbeta\ngamma\n").await.unwrap();
        (t, p)
    }

    #[tokio::test]
    async fn reads_with_hashlines() {
        let (t, p) = sample().await;
        let out = ReadTool
            .execute(
                json!({"path": p.file_name().unwrap().to_str().unwrap()}),
                &ToolContext::test(t.path()),
            )
            .await
            .unwrap();
        assert!(out.content.contains("1:"));
        assert!(out.content.contains("|alpha"));
        assert!(out.content.contains("|beta"));
    }

    #[tokio::test]
    async fn reads_plain_when_disabled() {
        let (t, p) = sample().await;
        let out = ReadTool
            .execute(
                json!({
                    "path": p.file_name().unwrap().to_str().unwrap(),
                    "hashlines": false
                }),
                &ToolContext::test(t.path()),
            )
            .await
            .unwrap();
        assert!(!out.content.contains('|'));
        assert!(out.content.contains("alpha"));
    }

    #[tokio::test]
    async fn range_slices() {
        let (t, p) = sample().await;
        let out = ReadTool
            .execute(
                json!({
                    "path": p.file_name().unwrap().to_str().unwrap(),
                    "start_line": 2,
                    "end_line": 2,
                    "hashlines": false
                }),
                &ToolContext::test(t.path()),
            )
            .await
            .unwrap();
        assert_eq!(out.content, "beta");
    }

    #[tokio::test]
    async fn missing_file_errors() {
        let t = TempDir::new().unwrap();
        let err = ReadTool
            .execute(
                json!({"path": "does-not-exist"}),
                &ToolContext::test(t.path()),
            )
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::Other(_)));
    }
}
