use harness_lifecycle::{ModelRegistry, Shutdown};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::time::{MissedTickBehavior, interval};
use tracing::{debug, info, warn};

pub const DEFAULT_ENDPOINT: &str = "http://localhost:11434";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
const POLL_INTERVAL: Duration = Duration::from_secs(5 * 60);

#[must_use]
pub fn spawn(
    endpoint: String,
    models: Arc<RwLock<ModelRegistry>>,
    shutdown: Shutdown,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let client = match reqwest::Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .build()
        {
            Ok(c) => c,
            Err(e) => {
                warn!(error = %e, "could not build ollama http client");
                return;
            }
        };
        let mut ticker = interval(POLL_INTERVAL);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                () = shutdown.cancelled() => break,
                _ = ticker.tick() => {
                    discover_once(&client, &endpoint, &models).await;
                }
            }
        }
        debug!("ollama discovery exiting");
    })
}

async fn discover_once(
    client: &reqwest::Client,
    endpoint: &str,
    models: &Arc<RwLock<ModelRegistry>>,
) {
    let url = format!("{}/api/tags", endpoint.trim_end_matches('/'));
    let resp = match client.get(&url).send().await {
        Ok(r) => r,
        Err(e) => {
            debug!(error = %e, "ollama unavailable");
            return;
        }
    };
    if !resp.status().is_success() {
        debug!(status = %resp.status(), "ollama returned non-success");
        return;
    }
    let body = match resp.text().await {
        Ok(b) => b,
        Err(e) => {
            warn!(error = %e, "ollama body read failed");
            return;
        }
    };
    let added = {
        let mut guard = models.write().await;
        match guard.ingest_ollama_tags(&body) {
            Ok(n) => n,
            Err(e) => {
                warn!(error = %e, "ollama tags parse failed");
                return;
            }
        }
    };
    if added > 0 {
        info!(models = added, "discovered ollama models");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn discover_once_ingests_models() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/tags"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "models": [
                    {"name": "llama3:8b"},
                    {"name": "codestral:22b"}
                ]
            })))
            .mount(&server)
            .await;
        let models = Arc::new(RwLock::new(ModelRegistry::new()));
        let client = reqwest::Client::new();
        discover_once(&client, &server.uri(), &models).await;
        assert_eq!(models.read().await.len(), 2);
    }

    #[tokio::test]
    async fn unavailable_endpoint_is_silent() {
        let models = Arc::new(RwLock::new(ModelRegistry::new()));
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_millis(100))
            .build()
            .unwrap();
        discover_once(&client, "http://127.0.0.1:1", &models).await;
        assert_eq!(models.read().await.len(), 0);
    }

    #[tokio::test]
    async fn non_200_is_ignored() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/tags"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&server)
            .await;
        let models = Arc::new(RwLock::new(ModelRegistry::new()));
        let client = reqwest::Client::new();
        discover_once(&client, &server.uri(), &models).await;
        assert_eq!(models.read().await.len(), 0);
    }
}
