use futures::{SinkExt, StreamExt};
use reqwest::Client as HttpClient;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::Duration;
use thiserror::Error;
use tokio::sync::{Mutex, oneshot};
use tokio_tungstenite::tungstenite::protocol::Message;
use tracing::debug;

#[derive(Debug, Error)]
pub enum CdpError {
    #[error("http: {0}")]
    Http(String),
    #[error("ws: {0}")]
    Ws(String),
    #[error("cdp error: {0}")]
    Server(String),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("timeout")]
    Timeout,
    #[error("disconnected")]
    Disconnected,
}

type WsStream =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;
type CdpWriter = futures::stream::SplitSink<WsStream, Message>;
type PendingMap = HashMap<i64, oneshot::Sender<Result<Value, String>>>;

pub struct CdpClient {
    write: Arc<Mutex<CdpWriter>>,
    pending: Arc<Mutex<PendingMap>>,
    next_id: AtomicI64,
    reader_handle: tokio::task::JoinHandle<()>,
}

#[derive(Debug, Deserialize)]
struct Target {
    #[serde(rename = "webSocketDebuggerUrl")]
    ws_url: Option<String>,
}

impl CdpClient {
    pub async fn connect_new_page(endpoint: &str) -> Result<Self, CdpError> {
        let http = HttpClient::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .map_err(|e| CdpError::Http(e.to_string()))?;
        let ws_url = Self::open_new_page(&http, endpoint).await?;
        Self::connect_ws(&ws_url).await
    }

    pub async fn connect_ws(ws_url: &str) -> Result<Self, CdpError> {
        let (stream, _) = tokio_tungstenite::connect_async(ws_url)
            .await
            .map_err(|e| CdpError::Ws(e.to_string()))?;
        let (write, mut read) = stream.split();
        let pending: Arc<Mutex<PendingMap>> = Arc::new(Mutex::new(HashMap::new()));
        let pending_clone = pending.clone();

        let reader_handle = tokio::spawn(async move {
            while let Some(msg) = read.next().await {
                let text = match msg {
                    Ok(Message::Text(t)) => t.to_string(),
                    Ok(Message::Close(_)) | Err(_) => break,
                    _ => continue,
                };
                let parsed: Value = match serde_json::from_str(&text) {
                    Ok(v) => v,
                    Err(e) => {
                        debug!(error = %e, "cdp parse");
                        continue;
                    }
                };
                if let Some(id) = parsed.get("id").and_then(Value::as_i64) {
                    let mut guard = pending_clone.lock().await;
                    if let Some(tx) = guard.remove(&id) {
                        if let Some(err) = parsed.get("error") {
                            let msg = err
                                .get("message")
                                .and_then(Value::as_str)
                                .unwrap_or("unknown")
                                .to_string();
                            tx.send(Err(msg)).ok();
                        } else {
                            let result = parsed.get("result").cloned().unwrap_or(Value::Null);
                            tx.send(Ok(result)).ok();
                        }
                    }
                }
            }
            let mut guard = pending_clone.lock().await;
            for (_, tx) in guard.drain() {
                tx.send(Err("socket closed".into())).ok();
            }
        });

        Ok(Self {
            write: Arc::new(Mutex::new(write)),
            pending,
            next_id: AtomicI64::new(1),
            reader_handle,
        })
    }

    async fn open_new_page(http: &HttpClient, endpoint: &str) -> Result<String, CdpError> {
        let url = format!("{}/json/new", endpoint.trim_end_matches('/'));
        let resp = http
            .put(url)
            .send()
            .await
            .map_err(|e| CdpError::Http(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(CdpError::Http(format!("{}", resp.status())));
        }
        let target: Target = resp
            .json()
            .await
            .map_err(|e| CdpError::Http(e.to_string()))?;
        target
            .ws_url
            .ok_or_else(|| CdpError::Http("no ws url".into()))
    }

    pub async fn send(&self, method: &str, params: Value) -> Result<Value, CdpError> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(id, tx);
        let body = json!({"id": id, "method": method, "params": params});
        {
            let mut w = self.write.lock().await;
            w.send(Message::Text(body.to_string().into()))
                .await
                .map_err(|e| CdpError::Ws(e.to_string()))?;
        }
        let response = tokio::time::timeout(Duration::from_secs(30), rx)
            .await
            .map_err(|_| {
                let pending = self.pending.clone();
                tokio::spawn(async move {
                    pending.lock().await.remove(&id);
                });
                CdpError::Timeout
            })?
            .map_err(|_| CdpError::Disconnected)?;
        response.map_err(CdpError::Server)
    }

    pub async fn close(&self) {
        let mut w = self.write.lock().await;
        w.close().await.ok();
        self.reader_handle.abort();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_deserializes_with_ws_url() {
        let t: Target = serde_json::from_str(
            r#"{"id":"x","type":"page","title":"t","url":"https://a","webSocketDebuggerUrl":"ws://x"}"#,
        )
        .unwrap();
        assert_eq!(t.ws_url.as_deref(), Some("ws://x"));
    }

    #[test]
    fn connect_new_page_reports_http_error() {
        let out = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(CdpClient::connect_new_page("http://127.0.0.1:1"));
        assert!(matches!(out, Err(CdpError::Http(_))));
    }
}
