use reqwest::Client;
use serde::Deserialize;
use std::time::Duration;
use thiserror::Error;
use tokio::time::Instant;
use tracing::debug;

#[derive(Debug, Error)]
pub enum UpdateError {
    #[error("network: {0}")]
    Network(String),
    #[error("http status {0}")]
    Status(u16),
    #[error("parse: {0}")]
    Parse(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateCheck {
    pub latest: String,
    pub current: String,
    pub available: bool,
    pub checked_at: std::time::SystemTime,
}

pub struct UpdateChecker {
    current: String,
    interval: Duration,
    last_check: Option<Instant>,
}

impl UpdateChecker {
    #[must_use]
    pub fn new(current: impl Into<String>, interval: Duration) -> Self {
        Self {
            current: current.into(),
            interval,
            last_check: None,
        }
    }

    #[must_use]
    pub fn should_check(&self, now: Instant) -> bool {
        match self.last_check {
            None => true,
            Some(prev) => now.saturating_duration_since(prev) >= self.interval,
        }
    }

    pub fn mark_checked(&mut self, now: Instant) {
        self.last_check = Some(now);
    }

    pub async fn check(&self, http: &Client, endpoint: &str) -> Result<UpdateCheck, UpdateError> {
        let resp = http
            .get(endpoint)
            .header("accept", "application/vnd.github+json")
            .header("user-agent", format!("harnessd/{}", self.current))
            .send()
            .await
            .map_err(|e| UpdateError::Network(e.to_string()))?;
        let status = resp.status();
        if !status.is_success() {
            return Err(UpdateError::Status(status.as_u16()));
        }
        let body: ReleaseBody = resp
            .json()
            .await
            .map_err(|e| UpdateError::Parse(e.to_string()))?;
        let latest = body.tag_name.trim_start_matches('v').trim().to_string();
        debug!(current = %self.current, latest = %latest, "update check");
        let available = !latest.is_empty() && latest != self.current;
        Ok(UpdateCheck {
            latest,
            current: self.current.clone(),
            available,
            checked_at: std::time::SystemTime::now(),
        })
    }

    #[must_use]
    pub fn is_idle(connected_clients: usize, inflight_llm_calls: usize) -> bool {
        connected_clients == 0 && inflight_llm_calls == 0
    }

    #[must_use]
    pub fn current(&self) -> &str {
        &self.current
    }
}

#[derive(Debug, Deserialize)]
struct ReleaseBody {
    tag_name: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn initial_check_due_immediately() {
        let c = UpdateChecker::new("0.1.0", Duration::from_secs(60));
        assert!(c.should_check(Instant::now()));
    }

    #[test]
    fn respects_interval() {
        let mut c = UpdateChecker::new("0.1.0", Duration::from_secs(60));
        let now = Instant::now();
        c.mark_checked(now);
        assert!(!c.should_check(now));
        assert!(!c.should_check(now + Duration::from_secs(30)));
        assert!(c.should_check(now + Duration::from_secs(61)));
    }

    #[test]
    fn idle_requires_both_zero() {
        assert!(UpdateChecker::is_idle(0, 0));
        assert!(!UpdateChecker::is_idle(1, 0));
        assert!(!UpdateChecker::is_idle(0, 1));
        assert!(!UpdateChecker::is_idle(3, 2));
    }

    #[tokio::test]
    async fn check_parses_latest_release() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/releases/latest"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "tag_name": "v0.2.0"
            })))
            .mount(&server)
            .await;

        let client = Client::new();
        let checker = UpdateChecker::new("0.1.0", Duration::from_secs(60));
        let endpoint = format!("{}/releases/latest", server.uri());
        let out = checker.check(&client, &endpoint).await.unwrap();
        assert_eq!(out.latest, "0.2.0");
        assert_eq!(out.current, "0.1.0");
        assert!(out.available);
    }

    #[tokio::test]
    async fn check_marks_not_available_when_versions_match() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/releases/latest"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "tag_name": "0.1.0"
            })))
            .mount(&server)
            .await;
        let client = Client::new();
        let checker = UpdateChecker::new("0.1.0", Duration::from_secs(60));
        let endpoint = format!("{}/releases/latest", server.uri());
        let out = checker.check(&client, &endpoint).await.unwrap();
        assert!(!out.available);
    }

    #[tokio::test]
    async fn check_reports_status_for_non_success() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/releases/latest"))
            .respond_with(ResponseTemplate::new(503))
            .mount(&server)
            .await;
        let client = Client::new();
        let checker = UpdateChecker::new("0.1.0", Duration::from_secs(60));
        let endpoint = format!("{}/releases/latest", server.uri());
        let err = checker.check(&client, &endpoint).await.unwrap_err();
        assert!(matches!(err, UpdateError::Status(503)));
    }
}
