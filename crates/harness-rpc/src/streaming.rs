use harness_proto::Notification;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub struct Sink {
    tx: Arc<mpsc::Sender<String>>,
}

impl Sink {
    #[must_use]
    pub fn new(tx: mpsc::Sender<String>) -> Self {
        Self { tx: Arc::new(tx) }
    }

    pub async fn send_raw(&self, payload: String) -> Result<(), SinkError> {
        self.tx
            .send(payload)
            .await
            .map_err(|_| SinkError::Disconnected)
    }

    pub async fn notify(
        &self,
        method: impl Into<String>,
        params: Option<Value>,
    ) -> Result<(), SinkError> {
        let note = Notification::new(method, params);
        let payload =
            serde_json::to_string(&note).map_err(|e| SinkError::Serialize(e.to_string()))?;
        self.send_raw(payload).await
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SinkError {
    #[error("client disconnected")]
    Disconnected,
    #[error("serialize: {0}")]
    Serialize(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn notify_writes_wire_format() {
        let (tx, mut rx) = mpsc::channel(4);
        let sink = Sink::new(tx);
        sink.notify("stream.delta", Some(json!({"content": "hi"})))
            .await
            .unwrap();
        let got = rx.recv().await.unwrap();
        assert!(got.contains("\"jsonrpc\":\"2.0\""));
        assert!(got.contains("\"method\":\"stream.delta\""));
        assert!(!got.contains("\"id\""));
    }

    #[tokio::test]
    async fn disconnected_sink_returns_error() {
        let (tx, rx) = mpsc::channel(1);
        let sink = Sink::new(tx);
        drop(rx);
        let err = sink.notify("x", None).await.unwrap_err();
        assert!(matches!(err, SinkError::Disconnected));
    }

    #[tokio::test]
    async fn send_raw_is_verbatim() {
        let (tx, mut rx) = mpsc::channel(2);
        let sink = Sink::new(tx);
        sink.send_raw("{\"test\":true}".into()).await.unwrap();
        assert_eq!(rx.recv().await.unwrap(), "{\"test\":true}");
    }
}
