use crate::provider::Provider;
use crate::stream::BoxStream;
use harness_llm_types::{ChatOptions, ChatRequest, ProviderError};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use thiserror::Error;
use tracing::{debug, info, warn};

#[derive(Debug, Error)]
pub enum PoolError {
    #[error(
        "no credential registered for provider `{family}` (required by model `{model}`).\n\
         fix it: `harness auth login {family}` — or pick another model with `harness model use <id>`"
    )]
    NoAvailable { model: String, family: String },

    #[error("all providers for `{family}` exhausted: {source}")]
    AllFailed {
        family: String,
        #[source]
        source: ProviderError,
    },

    #[error(
        "provider pool is empty — register a credential with `harness auth login`, \
         or start a local Ollama (`brew services start ollama`) and it will self-register"
    )]
    Empty,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ProviderHealth {
    Healthy,
    Throttled { until: Instant, failures: u32 },
    Quarantined { reason: String },
}

pub struct ProviderSlot {
    pub provider: Arc<dyn Provider>,
    pub health: Mutex<ProviderHealth>,
}

impl ProviderSlot {
    pub fn new(provider: Arc<dyn Provider>) -> Self {
        Self {
            provider,
            health: Mutex::new(ProviderHealth::Healthy),
        }
    }

    fn is_available(&self, now: Instant) -> bool {
        let h = self.health.lock();
        match &*h {
            ProviderHealth::Healthy => true,
            ProviderHealth::Throttled { until, .. } => *until <= now,
            ProviderHealth::Quarantined { .. } => false,
        }
    }

    fn mark_success(&self) {
        let mut h = self.health.lock();
        if !matches!(*h, ProviderHealth::Quarantined { .. }) {
            *h = ProviderHealth::Healthy;
        }
    }

    fn mark_failure(&self, err: &ProviderError, now: Instant) {
        let mut h = self.health.lock();

        match err {
            ProviderError::RateLimit { retry_after } => {
                let wait = retry_after.unwrap_or_else(|| Duration::from_secs(30));
                let new_failures = match &*h {
                    ProviderHealth::Throttled { failures, .. } => failures.saturating_add(1),
                    _ => 1,
                };
                *h = ProviderHealth::Throttled {
                    until: now + wait,
                    failures: new_failures,
                };
                info!(
                    wait_ms = u64::try_from(wait.as_millis()).unwrap_or(u64::MAX),
                    "slot throttled"
                );
            }
            ProviderError::AuthError => {
                *h = ProviderHealth::Quarantined {
                    reason: "auth rejected".into(),
                };
                warn!("slot quarantined: auth error");
            }
            ProviderError::ServerError { status, .. } if (500..600).contains(status) => {
                *h = ProviderHealth::Throttled {
                    until: now + Duration::from_secs(5),
                    failures: match &*h {
                        ProviderHealth::Throttled { failures, .. } => failures.saturating_add(1),
                        _ => 1,
                    },
                };
            }
            _ => {}
        }
    }
}

pub struct ProviderPool {
    slots: Vec<ProviderSlot>,
    cursors: Mutex<HashMap<String, usize>>,
}

impl ProviderPool {
    #[must_use]
    pub fn new(slots: Vec<ProviderSlot>) -> Self {
        Self {
            slots,
            cursors: Mutex::new(HashMap::new()),
        }
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.slots.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    fn pick_slot(&self, family: &str, now: Instant) -> Option<&ProviderSlot> {
        let candidates: Vec<&ProviderSlot> = self
            .slots
            .iter()
            .filter(|s| s.provider.family() == family && s.is_available(now))
            .collect();
        if candidates.is_empty() {
            return None;
        }
        let mut cursors = self.cursors.lock();
        let idx = cursors.entry(family.into()).or_insert(0);
        let choice = candidates[*idx % candidates.len()];
        *idx = idx.wrapping_add(1);
        Some(choice)
    }

    pub async fn chat(
        &self,
        family: &str,
        model: &str,
        request: ChatRequest,
        options: ChatOptions,
    ) -> Result<BoxStream, PoolError> {
        if self.slots.is_empty() {
            return Err(PoolError::Empty);
        }
        let now = Instant::now();
        let mut last_error: Option<ProviderError> = None;

        let healthy_count = self
            .slots
            .iter()
            .filter(|s| s.provider.family() == family && s.is_available(now))
            .count();

        for _ in 0..healthy_count.max(1) {
            let Some(slot) = self.pick_slot(family, now) else {
                break;
            };
            debug!(provider = %slot.provider.id(), "dispatching chat");
            match slot
                .provider
                .chat(model, request.clone(), options.clone())
                .await
            {
                Ok(stream) => {
                    slot.mark_success();
                    return Ok(stream);
                }
                Err(e) => {
                    slot.mark_failure(&e, now);
                    last_error = Some(e);
                }
            }
        }

        match last_error {
            Some(e) => Err(PoolError::AllFailed {
                family: family.to_string(),
                source: e,
            }),
            None => Err(PoolError::NoAvailable {
                model: model.to_string(),
                family: family.to_string(),
            }),
        }
    }

    #[must_use]
    pub fn inspect(&self) -> Vec<(String, ProviderHealth)> {
        self.slots
            .iter()
            .map(|s| (s.provider.id().to_string(), s.health.lock().clone()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use futures::stream;
    use harness_llm_types::{StreamEvent, Usage};
    use std::sync::atomic::{AtomicU32, Ordering};

    struct MockProvider {
        id: String,
        family: String,
        calls: AtomicU32,
        behavior: Mutex<Behavior>,
    }

    #[derive(Clone)]
    enum Behavior {
        Ok,
        RateLimit { retry_after: Option<Duration> },
        Auth,
        Server,
    }

    #[async_trait]
    impl Provider for MockProvider {
        fn id(&self) -> &str {
            &self.id
        }
        fn family(&self) -> &str {
            &self.family
        }
        async fn list_models(&self) -> Result<Vec<String>, ProviderError> {
            Ok(vec!["mock-1".into()])
        }
        async fn chat(
            &self,
            _model: &str,
            _request: ChatRequest,
            _options: ChatOptions,
        ) -> Result<BoxStream, ProviderError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let b = self.behavior.lock().clone();
            match b {
                Behavior::Ok => {
                    let events = vec![
                        Ok(StreamEvent::Delta {
                            content: "hi".into(),
                        }),
                        Ok(StreamEvent::Done {
                            usage: Usage::default(),
                        }),
                    ];
                    Ok(Box::pin(stream::iter(events)))
                }
                Behavior::RateLimit { retry_after } => {
                    Err(ProviderError::RateLimit { retry_after })
                }
                Behavior::Auth => Err(ProviderError::AuthError),
                Behavior::Server => Err(ProviderError::ServerError {
                    status: 502,
                    message: "boom".into(),
                }),
            }
        }
    }

    fn slot(id: &str, family: &str, behavior: Behavior) -> ProviderSlot {
        ProviderSlot::new(Arc::new(MockProvider {
            id: id.into(),
            family: family.into(),
            calls: AtomicU32::new(0),
            behavior: Mutex::new(behavior),
        }))
    }

    fn req() -> (ChatRequest, ChatOptions) {
        (
            ChatRequest {
                system: None,
                messages: vec![],
                tools: vec![],
            },
            ChatOptions::default(),
        )
    }

    async fn chat_err(
        pool: &ProviderPool,
        family: &str,
        model: &str,
        r: ChatRequest,
        o: ChatOptions,
    ) -> PoolError {
        match pool.chat(family, model, r, o).await {
            Ok(_) => panic!("expected error"),
            Err(e) => e,
        }
    }

    #[tokio::test]
    async fn empty_pool_errors() {
        let pool = ProviderPool::new(vec![]);
        let (r, o) = req();
        let err = chat_err(&pool, "anthropic", "m", r, o).await;
        assert!(matches!(err, PoolError::Empty));
    }

    #[tokio::test]
    async fn single_healthy_slot_succeeds() {
        let pool = ProviderPool::new(vec![slot("a", "anthropic", Behavior::Ok)]);
        let (r, o) = req();
        let _stream = pool.chat("anthropic", "m", r, o).await.unwrap();
    }

    #[tokio::test]
    async fn rate_limited_slot_is_skipped_next_call() {
        let pool = ProviderPool::new(vec![
            slot(
                "rl",
                "anthropic",
                Behavior::RateLimit {
                    retry_after: Some(Duration::from_secs(30)),
                },
            ),
            slot("ok", "anthropic", Behavior::Ok),
        ]);

        let (r, o) = req();
        drop(pool.chat("anthropic", "m", r, o).await.unwrap());

        let inspection = pool.inspect();
        let rl = inspection.iter().find(|(id, _)| id == "rl").unwrap();
        assert!(matches!(rl.1, ProviderHealth::Throttled { .. }));
    }

    #[tokio::test]
    async fn auth_error_quarantines_slot() {
        let pool = ProviderPool::new(vec![
            slot("bad", "anthropic", Behavior::Auth),
            slot("good", "anthropic", Behavior::Ok),
        ]);
        let (r, o) = req();
        drop(pool.chat("anthropic", "m", r, o).await.unwrap());
        let inspection = pool.inspect();
        let bad = inspection.iter().find(|(id, _)| id == "bad").unwrap();
        assert!(matches!(bad.1, ProviderHealth::Quarantined { .. }));
    }

    #[tokio::test]
    async fn all_failing_returns_all_failed() {
        let pool = ProviderPool::new(vec![
            slot("a", "anthropic", Behavior::Auth),
            slot("b", "anthropic", Behavior::Auth),
        ]);
        let (r, o) = req();
        let err = chat_err(&pool, "anthropic", "m", r, o).await;
        assert!(matches!(
            err,
            PoolError::AllFailed {
                source: ProviderError::AuthError,
                ..
            }
        ));
    }

    #[tokio::test]
    async fn server_error_throttles_briefly() {
        let pool = ProviderPool::new(vec![slot("x", "anthropic", Behavior::Server)]);
        let (r, o) = req();
        pool.chat("anthropic", "m", r, o).await.ok();
        let inspection = pool.inspect();
        match &inspection[0].1 {
            ProviderHealth::Throttled { until, .. } => {
                let remaining = until.saturating_duration_since(Instant::now());
                assert!(remaining <= Duration::from_secs(6));
                assert!(remaining >= Duration::from_millis(500));
            }
            other => panic!("expected throttled, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn round_robin_distributes_calls() {
        let pool = ProviderPool::new(vec![
            slot("a", "anthropic", Behavior::Ok),
            slot("b", "anthropic", Behavior::Ok),
        ]);
        let (r, o) = req();
        for _ in 0..4 {
            drop(
                pool.chat("anthropic", "m", r.clone(), o.clone())
                    .await
                    .unwrap(),
            );
        }
        for (_, h) in pool.inspect() {
            assert_eq!(h, ProviderHealth::Healthy);
        }
    }

    #[tokio::test]
    async fn family_isolation() {
        let pool = ProviderPool::new(vec![
            slot("a", "anthropic", Behavior::Ok),
            slot("o", "openai", Behavior::Ok),
        ]);
        let (r, o) = req();
        drop(
            pool.chat("anthropic", "m", r.clone(), o.clone())
                .await
                .unwrap(),
        );
        drop(pool.chat("openai", "m", r, o).await.unwrap());
    }

    #[tokio::test]
    async fn unknown_family_no_available() {
        let pool = ProviderPool::new(vec![slot("a", "anthropic", Behavior::Ok)]);
        let (r, o) = req();
        let err = chat_err(&pool, "nobody", "m", r, o).await;
        assert!(matches!(err, PoolError::NoAvailable { .. }));
    }
}
