use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::Duration;
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::{Mutex, oneshot};
use tokio::task::JoinHandle;
use tracing::{debug, info, warn};

pub const MCP_PROTOCOL_VERSION: &str = "2025-06-18";

#[derive(Debug, Error)]
pub enum McpError {
    #[error("spawn: {0}")]
    Spawn(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("server error: {0}")]
    Server(String),
    #[error("server exited before response")]
    Disconnected,
    #[error("timeout")]
    Timeout,
}

#[derive(Debug, Clone)]
pub struct McpServerConfig {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    pub cwd: Option<std::path::PathBuf>,
}

// IMPLEMENTS: D-009
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerTool {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default, rename = "inputSchema")]
    pub input_schema: Value,
}

type Pending = HashMap<i64, oneshot::Sender<Result<Value, String>>>;

pub struct ManagedServer {
    config: McpServerConfig,
    child: Arc<Mutex<Option<Child>>>,
    stdin: Arc<Mutex<Option<ChildStdin>>>,
    pending: Arc<Mutex<Pending>>,
    next_id: AtomicI64,
    reader: Mutex<Option<JoinHandle<()>>>,
    tools: Vec<ServerTool>,
}

impl ManagedServer {
    pub async fn spawn(config: McpServerConfig) -> Result<Self, McpError> {
        let mut cmd = Command::new(&config.command);
        cmd.args(&config.args);
        if let Some(dir) = &config.cwd {
            cmd.current_dir(dir);
        }
        let mut child = cmd
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| McpError::Spawn(e.to_string()))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| McpError::Spawn("no stdin".into()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| McpError::Spawn("no stdout".into()))?;
        let stderr = child.stderr.take();

        let pending: Arc<Mutex<Pending>> = Arc::new(Mutex::new(HashMap::new()));
        let pending_clone = pending.clone();
        let name_for_log = config.name.clone();
        let reader = tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            loop {
                match lines.next_line().await {
                    Ok(Some(line)) => {
                        if line.trim().is_empty() {
                            continue;
                        }
                        let value: Value = match serde_json::from_str(&line) {
                            Ok(v) => v,
                            Err(e) => {
                                warn!(server = %name_for_log, error = %e, "mcp parse error");
                                continue;
                            }
                        };
                        if let Some(id) = value.get("id").and_then(Value::as_i64) {
                            let mut guard = pending_clone.lock().await;
                            if let Some(tx) = guard.remove(&id) {
                                if let Some(err) = value.get("error") {
                                    tx.send(Err(err.to_string())).ok();
                                } else if let Some(result) = value.get("result").cloned() {
                                    tx.send(Ok(result)).ok();
                                } else {
                                    tx.send(Err("no result".into())).ok();
                                }
                            }
                        } else {
                            debug!(server = %name_for_log, ?value, "mcp notification");
                        }
                    }
                    Ok(None) => {
                        info!(server = %name_for_log, "mcp server closed stdout");
                        break;
                    }
                    Err(e) => {
                        warn!(server = %name_for_log, error = %e, "mcp read error");
                        break;
                    }
                }
            }
            let mut guard = pending_clone.lock().await;
            for (_, tx) in guard.drain() {
                tx.send(Err("server disconnected".into())).ok();
            }
        });

        if let Some(err) = stderr {
            let name = config.name.clone();
            tokio::spawn(async move {
                let mut lines = BufReader::new(err).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    debug!(server = %name, "{line}");
                }
            });
        }

        let server = Self {
            config: config.clone(),
            child: Arc::new(Mutex::new(Some(child))),
            stdin: Arc::new(Mutex::new(Some(stdin))),
            pending,
            next_id: AtomicI64::new(1),
            reader: Mutex::new(Some(reader)),
            tools: Vec::new(),
        };

        server.initialize().await?;
        let tools = server.list_tools().await?;
        Ok(Self { tools, ..server })
    }

    #[must_use]
    pub fn config(&self) -> &McpServerConfig {
        &self.config
    }

    #[must_use]
    pub fn tools(&self) -> &[ServerTool] {
        &self.tools
    }

    pub async fn call_tool(&self, name: &str, input: Value) -> Result<Value, McpError> {
        let result = self
            .request(
                "tools/call",
                json!({
                    "name": name,
                    "arguments": input,
                }),
            )
            .await?;
        Ok(result)
    }

    pub async fn shutdown(&self) {
        {
            let mut stdin = self.stdin.lock().await;
            if let Some(mut s) = stdin.take() {
                s.shutdown().await.ok();
            }
        }
        if let Some(mut child) = self.child.lock().await.take() {
            child.kill().await.ok();
            child.wait().await.ok();
        }
        if let Some(handle) = self.reader.lock().await.take() {
            handle.await.ok();
        }
    }

    // IMPLEMENTS: D-009
    async fn initialize(&self) -> Result<(), McpError> {
        self.request(
            "initialize",
            json!({
                "protocolVersion": MCP_PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": {"name": "harness", "version": env!("CARGO_PKG_VERSION")},
            }),
        )
        .await?;
        self.notify("notifications/initialized", json!({})).await?;
        Ok(())
    }

    // IMPLEMENTS: D-073
    async fn list_tools(&self) -> Result<Vec<ServerTool>, McpError> {
        let v = self.request("tools/list", json!({})).await?;
        let tools = v
            .get("tools")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let mut out = Vec::with_capacity(tools.len());
        for t in tools {
            let tool: ServerTool = serde_json::from_value(t)?;
            // Quarantine tools whose schema breaks the D-073 envelope —
            // serialize the validation off the async runtime to avoid
            // blocking other clients on a malicious payload.
            let schema = tool.input_schema.clone();
            let name = tool.name.clone();
            let server_name = self.config.name.clone();
            let validation =
                tokio::task::spawn_blocking(move || crate::schema_validator::validate(&schema))
                    .await
                    .map_err(|e| McpError::Server(format!("validator join: {e}")))?;
            if let Err(e) = validation {
                warn!(
                    server = %server_name,
                    tool = %name,
                    error = %e,
                    "quarantined mcp tool: invalid inputSchema"
                );
                continue;
            }
            out.push(tool);
        }
        Ok(out)
    }

    async fn request(&self, method: &str, params: Value) -> Result<Value, McpError> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(id, tx);

        let body = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        self.write_line(&body).await?;

        let response = tokio::time::timeout(Duration::from_secs(60), rx)
            .await
            .map_err(|_| {
                let pending = self.pending.clone();
                tokio::spawn(async move {
                    pending.lock().await.remove(&id);
                });
                McpError::Timeout
            })?
            .map_err(|_| McpError::Disconnected)?;
        response.map_err(McpError::Server)
    }

    async fn notify(&self, method: &str, params: Value) -> Result<(), McpError> {
        let body = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });
        self.write_line(&body).await
    }

    async fn write_line(&self, value: &Value) -> Result<(), McpError> {
        let mut stdin_guard = self.stdin.lock().await;
        let stdin = stdin_guard
            .as_mut()
            .ok_or_else(|| McpError::Server("stdin closed".into()))?;
        let text = serde_json::to_string(value)?;
        stdin.write_all(text.as_bytes()).await?;
        stdin.write_all(b"\n").await?;
        stdin.flush().await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;
    use tempfile::TempDir;

    fn fake_server_script(tmp: &TempDir) -> std::path::PathBuf {
        let path = tmp.path().join("fake_mcp.sh");
        let mut f = fs::File::create(&path).unwrap();
        f.write_all(
            br#"#!/usr/bin/env bash
while IFS= read -r line; do
    method=$(printf '%s' "$line" | sed -n 's/.*"method":"\([^"]*\)".*/\1/p')
    id=$(printf '%s' "$line" | sed -n 's/.*"id":\([0-9]*\).*/\1/p')
    case "$method" in
        initialize)
            printf '{"jsonrpc":"2.0","id":%s,"result":{"protocolVersion":"2024-11-05","capabilities":{},"serverInfo":{"name":"fake","version":"0.0.1"}}}\n' "$id"
            ;;
        notifications/initialized)
            :
            ;;
        tools/list)
            printf '{"jsonrpc":"2.0","id":%s,"result":{"tools":[{"name":"echo","description":"echo","inputSchema":{"type":"object"}}]}}\n' "$id"
            ;;
        tools/call)
            printf '{"jsonrpc":"2.0","id":%s,"result":{"content":[{"type":"text","text":"echoed"}]}}\n' "$id"
            ;;
        *)
            printf '{"jsonrpc":"2.0","id":%s,"error":{"code":-32601,"message":"method not found"}}\n' "$id"
            ;;
    esac
done
"#,
        )
        .unwrap();
        drop(f);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut p = fs::metadata(&path).unwrap().permissions();
            p.set_mode(0o755);
            fs::set_permissions(&path, p).unwrap();
        }
        path
    }

    #[tokio::test]
    async fn initialize_and_list_tools() {
        let tmp = TempDir::new().unwrap();
        let path = fake_server_script(&tmp);
        let server = ManagedServer::spawn(McpServerConfig {
            name: "fake".into(),
            command: "bash".into(),
            args: vec![path.to_string_lossy().into()],
            cwd: None,
        })
        .await
        .unwrap();
        assert_eq!(server.tools().len(), 1);
        assert_eq!(server.tools()[0].name, "echo");
        server.shutdown().await;
    }

    #[tokio::test]
    async fn call_tool_returns_content() {
        let tmp = TempDir::new().unwrap();
        let path = fake_server_script(&tmp);
        let server = ManagedServer::spawn(McpServerConfig {
            name: "fake".into(),
            command: "bash".into(),
            args: vec![path.to_string_lossy().into()],
            cwd: None,
        })
        .await
        .unwrap();
        let out = server.call_tool("echo", json!({})).await.unwrap();
        let content = out.get("content").and_then(Value::as_array).unwrap();
        let text = content[0].get("text").and_then(Value::as_str).unwrap();
        assert_eq!(text, "echoed");
        server.shutdown().await;
    }
}
