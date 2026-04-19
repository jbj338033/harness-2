use anyhow::{Result, anyhow, bail};
use harness_proto::{Id, Request, Response, ResponsePayload};
use serde_json::Value;
use std::path::PathBuf;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::time::{Duration, timeout};

#[must_use]
pub fn socket_path() -> PathBuf {
    if let Ok(p) = std::env::var("HARNESS_DATA_DIR") {
        return PathBuf::from(p).join("harness.sock");
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".harness").join("harness.sock")
}

pub async fn call(method: &str, params: Option<Value>) -> Result<Value> {
    let path = socket_path();
    let stream = UnixStream::connect(&path)
        .await
        .map_err(|e| anyhow!("connect {}: {e}", path.display()))?;
    let (r, mut w) = stream.into_split();
    let req = Request::new(Id::Number(1), method, params);
    let text = serde_json::to_string(&req)?;
    w.write_all(text.as_bytes()).await?;
    w.write_all(b"\n").await?;
    w.flush().await?;

    let mut lines = BufReader::new(r).lines();
    let line = timeout(Duration::from_secs(10), lines.next_line())
        .await
        .map_err(|_| anyhow!("daemon did not respond within 10s"))??
        .ok_or_else(|| anyhow!("daemon closed the socket"))?;
    let resp: Response = serde_json::from_str(&line)?;
    match resp.payload {
        ResponsePayload::Result(v) => Ok(v),
        ResponsePayload::Error(e) => bail!("daemon error {}: {}", e.code, e.message),
    }
}
