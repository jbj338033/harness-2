use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::Duration;
use thiserror::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::{Mutex, oneshot};
use tokio::task::JoinHandle;
use tracing::{debug, warn};

#[derive(Debug, Error)]
pub enum LspError {
    #[error("spawn: {0}")]
    Spawn(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("server error: {0}")]
    Server(String),
    #[error("disconnected")]
    Disconnected,
    #[error("timeout")]
    Timeout,
}

#[derive(Debug, Clone)]
pub struct LspConfig {
    pub command: String,
    pub args: Vec<String>,
    pub root: PathBuf,
}

type PendingMap = HashMap<i64, oneshot::Sender<Result<Value, String>>>;

pub struct LspClient {
    stdin: Arc<Mutex<Option<ChildStdin>>>,
    child: Arc<Mutex<Option<Child>>>,
    pending: Arc<Mutex<PendingMap>>,
    diagnostics: Arc<Mutex<HashMap<String, Vec<Value>>>>,
    next_id: AtomicI64,
    reader: Mutex<Option<JoinHandle<()>>>,
}

impl LspClient {
    pub async fn spawn(config: LspConfig) -> Result<Self, LspError> {
        let mut cmd = Command::new(&config.command);
        cmd.args(&config.args);
        cmd.current_dir(&config.root);
        let mut child = cmd
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| LspError::Spawn(e.to_string()))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| LspError::Spawn("no stdin".into()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| LspError::Spawn("no stdout".into()))?;
        let stderr = child.stderr.take();

        let pending: Arc<Mutex<PendingMap>> = Arc::new(Mutex::new(HashMap::new()));
        let diagnostics: Arc<Mutex<HashMap<String, Vec<Value>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let pending_clone = pending.clone();
        let diagnostics_clone = diagnostics.clone();

        let reader = tokio::spawn(async move {
            let mut buf = BufReader::new(stdout);
            loop {
                match read_message(&mut buf).await {
                    Ok(None) => break,
                    Ok(Some(msg)) => dispatch(msg, &pending_clone, &diagnostics_clone).await,
                    Err(e) => {
                        warn!(error = %e, "lsp read error");
                        break;
                    }
                }
            }
            let mut guard = pending_clone.lock().await;
            for (_, tx) in guard.drain() {
                tx.send(Err("lsp server closed".into())).ok();
            }
        });

        if let Some(err) = stderr {
            use tokio::io::AsyncBufReadExt;
            tokio::spawn(async move {
                let mut buf = BufReader::new(err);
                let mut text = String::new();
                loop {
                    text.clear();
                    match buf.read_line(&mut text).await {
                        Ok(0) | Err(_) => break,
                        Ok(_) => debug!("lsp stderr: {}", text.trim_end()),
                    }
                }
            });
        }

        let client = Self {
            stdin: Arc::new(Mutex::new(Some(stdin))),
            child: Arc::new(Mutex::new(Some(child))),
            pending,
            diagnostics,
            next_id: AtomicI64::new(1),
            reader: Mutex::new(Some(reader)),
        };

        client.initialize(&config.root).await?;
        Ok(client)
    }

    pub async fn request(&self, method: &str, params: Value) -> Result<Value, LspError> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(id, tx);
        let body = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        self.write_message(&body).await?;
        let response = tokio::time::timeout(Duration::from_secs(30), rx)
            .await
            .map_err(|_| {
                let pending = self.pending.clone();
                tokio::spawn(async move {
                    pending.lock().await.remove(&id);
                });
                LspError::Timeout
            })?
            .map_err(|_| LspError::Disconnected)?;
        response.map_err(LspError::Server)
    }

    pub async fn notify(&self, method: &str, params: Value) -> Result<(), LspError> {
        let body = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });
        self.write_message(&body).await
    }

    pub async fn diagnostics(&self, uri: &str) -> Vec<Value> {
        self.diagnostics
            .lock()
            .await
            .get(uri)
            .cloned()
            .unwrap_or_default()
    }

    pub async fn shutdown(&self) {
        self.request("shutdown", Value::Null).await.ok();
        self.notify("exit", Value::Null).await.ok();
        let mut stdin = self.stdin.lock().await;
        if let Some(mut s) = stdin.take() {
            s.shutdown().await.ok();
        }
        if let Some(mut c) = self.child.lock().await.take() {
            c.kill().await.ok();
            c.wait().await.ok();
        }
        if let Some(h) = self.reader.lock().await.take() {
            h.await.ok();
        }
    }

    async fn initialize(&self, root: &std::path::Path) -> Result<(), LspError> {
        let uri = path_to_uri(root);
        self.request(
            "initialize",
            json!({
                "processId": std::process::id(),
                "rootUri": uri,
                "capabilities": {
                    "textDocument": {
                        "publishDiagnostics": {"relatedInformation": true},
                        "definition": {"linkSupport": true},
                        "references": {},
                        "rename": {"prepareSupport": false}
                    }
                }
            }),
        )
        .await?;
        self.notify("initialized", json!({})).await?;
        Ok(())
    }

    async fn write_message(&self, value: &Value) -> Result<(), LspError> {
        let text = serde_json::to_string(value)?;
        let mut guard = self.stdin.lock().await;
        let stdin = guard
            .as_mut()
            .ok_or_else(|| LspError::Server("stdin closed".into()))?;
        let header = format!(
            "Content-Length: {}\r\nContent-Type: application/vscode-jsonrpc; charset=utf-8\r\n\r\n",
            text.len()
        );
        stdin.write_all(header.as_bytes()).await?;
        stdin.write_all(text.as_bytes()).await?;
        stdin.flush().await?;
        Ok(())
    }
}

async fn dispatch(
    msg: Value,
    pending: &Mutex<HashMap<i64, oneshot::Sender<Result<Value, String>>>>,
    diagnostics: &Mutex<HashMap<String, Vec<Value>>>,
) {
    if let Some(id) = msg.get("id").and_then(Value::as_i64) {
        let mut guard = pending.lock().await;
        if let Some(tx) = guard.remove(&id) {
            if let Some(err) = msg.get("error") {
                let text = err
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown")
                    .to_string();
                tx.send(Err(text)).ok();
            } else {
                tx.send(Ok(msg.get("result").cloned().unwrap_or(Value::Null)))
                    .ok();
            }
        }
        return;
    }
    let method = msg.get("method").and_then(Value::as_str).unwrap_or("");
    if method == "textDocument/publishDiagnostics"
        && let Some(params) = msg.get("params")
    {
        let uri = params
            .get("uri")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let diags = params
            .get("diagnostics")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        diagnostics.lock().await.insert(uri, diags);
    }
}

async fn read_message<R>(reader: &mut BufReader<R>) -> Result<Option<Value>, LspError>
where
    R: tokio::io::AsyncRead + Unpin,
{
    use tokio::io::AsyncBufReadExt;

    let mut length: Option<usize> = None;
    loop {
        let mut line = String::new();
        let read = reader.read_line(&mut line).await?;
        if read == 0 {
            return Ok(None);
        }
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            break;
        }
        if let Some(rest) = trimmed.strip_prefix("Content-Length:") {
            length = Some(
                rest.trim()
                    .parse::<usize>()
                    .map_err(|e| LspError::Server(format!("bad Content-Length: {e}")))?,
            );
        }
    }

    let Some(length) = length else {
        return Err(LspError::Server("no Content-Length".into()));
    };
    let mut body = vec![0u8; length];
    reader.read_exact(&mut body).await?;
    let parsed: Value = serde_json::from_slice(&body)?;
    Ok(Some(parsed))
}

#[must_use]
pub fn path_to_uri(p: &std::path::Path) -> String {
    let abs = if p.is_absolute() {
        p.to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_default().join(p)
    };
    let s = abs.to_string_lossy().replace('\\', "/");
    if s.starts_with('/') {
        format!("file://{s}")
    } else {
        format!("file:///{s}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn path_to_uri_works_for_unix_absolute() {
        let uri = path_to_uri(Path::new("/tmp/proj"));
        assert_eq!(uri, "file:///tmp/proj");
    }

    #[test]
    fn read_message_parses_headers_and_body() {
        let payload = r#"{"jsonrpc":"2.0","id":1,"result":null}"#;
        let framed = format!("Content-Length: {}\r\n\r\n{}", payload.len(), payload);
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let mut buf = BufReader::new(framed.as_bytes());
            let msg = read_message(&mut buf).await.unwrap().unwrap();
            assert_eq!(msg["id"], 1);
        });
    }

    #[test]
    fn read_message_returns_none_on_empty_stream() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let mut buf = BufReader::new(&b""[..]);
            let got = read_message(&mut buf).await.unwrap();
            assert!(got.is_none());
        });
    }
}
