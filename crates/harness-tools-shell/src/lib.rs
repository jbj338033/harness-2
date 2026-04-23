// IMPLEMENTS: D-096, D-097, D-098, D-099, D-100, D-101, D-103, D-104, D-105, D-106, D-107, D-108, D-109, D-110, D-112
use async_trait::async_trait;
use harness_tools::{
    ApprovalVerdict, Sandbox, SandboxPolicy, Tool, ToolContext, ToolError, ToolOutput,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::process::Stdio;
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::time::timeout;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(120);
const OUTPUT_LIMIT_BYTES: usize = 256 * 1024;

pub struct BashTool;

#[derive(Debug, Deserialize)]
struct BashInput {
    command: String,
    #[serde(default)]
    cwd: Option<String>,
    #[serde(default)]
    timeout_secs: Option<u64>,
}

#[async_trait]
impl Tool for BashTool {
    fn name(&self) -> &'static str {
        "bash"
    }

    fn description(&self) -> &'static str {
        "Execute a shell command. Output is captured from stdout+stderr \
         up to 256KB. Commands run with a configurable timeout.\n\
         USE: running tests, builds, git, ad-hoc scripts.\n\
         DO NOT USE: file reads/writes (use `read`/`write`/`edit`); \
         content search (use `grep`); destructive operations without user \
         intent (the sandbox will block many)."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["command"],
            "properties": {
                "command": { "type": "string" },
                "cwd": { "type": "string" },
                "timeout_secs": { "type": "integer", "minimum": 1, "default": 120 }
            }
        })
    }

    async fn execute(&self, input: Value, ctx: &ToolContext) -> Result<ToolOutput, ToolError> {
        let args: BashInput =
            serde_json::from_value(input).map_err(|e| ToolError::Input(e.to_string()))?;

        match Sandbox::evaluate_command(&args.command) {
            SandboxPolicy::Deny { reason } => {
                return Err(ToolError::Denied(format!(
                    "command refused by sandbox: {reason}"
                )));
            }
            SandboxPolicy::Confirm { reason } => {
                let pattern = approval_pattern(&args.command);
                let verdict = match ctx.approval.as_ref() {
                    Some(req) => {
                        req.request(ctx.session, args.command.clone(), pattern, reason.clone())
                            .await
                    }
                    None => ApprovalVerdict::Denied,
                };
                if verdict == ApprovalVerdict::Denied {
                    return Err(ToolError::Denied(format!(
                        "user denied command requiring approval: {reason}"
                    )));
                }
            }
            SandboxPolicy::Allow => {}
        }

        let cwd = args
            .cwd
            .as_deref()
            .map_or_else(|| ctx.cwd.clone(), |c| crate::resolve(&ctx.cwd, c));
        let deadline =
            Duration::from_secs(args.timeout_secs.unwrap_or(120)).min(Duration::from_secs(3600));
        let deadline = if deadline.is_zero() {
            DEFAULT_TIMEOUT
        } else {
            deadline
        };

        let mut cmd = Command::new("bash");
        cmd.arg("-c")
            .arg(&args.command)
            .current_dir(&cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        let mut child = cmd
            .spawn()
            .map_err(|e| ToolError::Other(format!("spawn bash: {e}")))?;

        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        let out_task = tokio::spawn(read_capped(stdout));
        let err_task = tokio::spawn(read_capped(stderr));

        let wait_status = timeout(deadline, child.wait()).await;
        let status = match wait_status {
            Ok(Ok(s)) => s,
            Ok(Err(e)) => return Err(ToolError::Other(format!("bash wait: {e}"))),
            Err(_) => {
                child.start_kill().ok();
                return Ok(ToolOutput::err(format!(
                    "command timed out after {}s",
                    deadline.as_secs()
                )));
            }
        };

        let stdout_s = out_task.await.unwrap_or_default();
        let stderr_s = err_task.await.unwrap_or_default();
        let exit = status.code().unwrap_or(-1);

        let mut body = String::new();
        if !stdout_s.is_empty() {
            body.push_str(&stdout_s);
        }
        if !stderr_s.is_empty() {
            if !body.is_empty() && !body.ends_with('\n') {
                body.push('\n');
            }
            body.push_str(&stderr_s);
        }
        if body.is_empty() {
            body.push_str("(no output)");
        }

        let out = if status.success() {
            ToolOutput::ok(body)
        } else {
            ToolOutput::err(body)
        };
        Ok(out.with_metadata(json!({ "exit_code": exit })))
    }
}

fn approval_pattern(command: &str) -> String {
    let trimmed = command.trim();
    let mut tokens: Vec<&str> = Vec::new();
    for tok in trimmed.split_whitespace() {
        if tok.starts_with('-') {
            break;
        }
        tokens.push(tok);
        if tokens.len() == 2 {
            break;
        }
    }
    if tokens.is_empty() {
        trimmed.to_string()
    } else {
        tokens.join(" ")
    }
}

mod fs_resolve {
    use std::path::{Path, PathBuf};
    pub fn resolve(cwd: &Path, path: &str) -> PathBuf {
        let p = Path::new(path);
        if p.is_absolute() {
            p.to_path_buf()
        } else {
            cwd.join(p)
        }
    }
}
use fs_resolve::resolve;

async fn read_capped<R: AsyncReadExt + Unpin>(reader: Option<R>) -> String {
    let Some(mut reader) = reader else {
        return String::new();
    };
    let mut buf = Vec::with_capacity(8 * 1024);
    let mut chunk = [0u8; 4096];
    loop {
        match reader.read(&mut chunk).await {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                let remaining = OUTPUT_LIMIT_BYTES.saturating_sub(buf.len());
                if remaining == 0 {
                    break;
                }
                let take = n.min(remaining);
                buf.extend_from_slice(&chunk[..take]);
                if take < n {
                    buf.extend_from_slice(b"\n... [output truncated]\n");
                    break;
                }
            }
        }
    }
    String::from_utf8_lossy(&buf).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn runs_simple_command() {
        let t = TempDir::new().unwrap();
        let out = BashTool
            .execute(
                json!({"command": "echo hello"}),
                &ToolContext::test(t.path()),
            )
            .await
            .unwrap();
        assert!(!out.is_error);
        assert!(out.content.contains("hello"));
    }

    #[tokio::test]
    async fn captures_non_zero_exit_as_is_error() {
        let t = TempDir::new().unwrap();
        let out = BashTool
            .execute(json!({"command": "false"}), &ToolContext::test(t.path()))
            .await
            .unwrap();
        assert!(out.is_error);
    }

    #[tokio::test]
    async fn sandbox_denies_rm_rf_root() {
        let t = TempDir::new().unwrap();
        let err = BashTool
            .execute(json!({"command": "rm -rf /"}), &ToolContext::test(t.path()))
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::Denied(_)));
    }

    #[tokio::test]
    async fn respects_timeout() {
        let t = TempDir::new().unwrap();
        let out = BashTool
            .execute(
                json!({"command": "sleep 5", "timeout_secs": 1}),
                &ToolContext::test(t.path()),
            )
            .await
            .unwrap();
        assert!(out.is_error);
        assert!(out.content.contains("timed out"));
    }

    #[tokio::test]
    async fn runs_in_given_cwd() {
        let t = TempDir::new().unwrap();
        tokio::fs::write(t.path().join("marker"), "x")
            .await
            .unwrap();
        let out = BashTool
            .execute(json!({"command": "ls"}), &ToolContext::test(t.path()))
            .await
            .unwrap();
        assert!(out.content.contains("marker"));
    }
}
