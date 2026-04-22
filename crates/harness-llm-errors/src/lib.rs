// IMPLEMENTS: D-129
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// One vocabulary every provider wrapper normalises into. Lets the turn
/// loop pick a retry / abort policy without knowing which provider raised
/// the error.
#[derive(Debug, Clone, Error, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NormalizedError {
    /// Auth missing, expired, or rejected. Never auto-retried — needs the
    /// user to fix credentials.
    #[error("auth: {message}")]
    Auth { message: String },

    /// Rate limited. Caller should back off and retry.
    #[error("rate limited (retry after {retry_after_ms} ms): {message}")]
    RateLimit {
        retry_after_ms: u64,
        message: String,
    },

    /// Model returned reasoning the provider considers opaque (eg. OpenAI
    /// summarised vs raw). One-shot retry with `reasoning=plain` is
    /// allowed; a second occurrence aborts the turn (D-129).
    #[error("invalid opaque reasoning: {message}")]
    InvalidOpaqueReasoning { message: String },

    /// Generic transient — retryable once with backoff.
    #[error("transient: {message}")]
    Transient { message: String },

    /// Generic permanent — abort, surface to user.
    #[error("permanent: {message}")]
    Permanent { message: String },

    /// Context window exceeded. Caller should compact or truncate before
    /// retry (no auto-retry without compaction).
    #[error("context overflow: {message}")]
    ContextOverflow { message: String },

    /// The requested model id is unknown to the provider — typically a
    /// deprecation. Surfaces the deprecation feed (Q-MODEL-DEPRECATION-FEED).
    #[error("unknown model {model}: {message}")]
    UnknownModel { model: String, message: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryDecision {
    /// Retry once, exactly as before.
    RetryOnce,
    /// Retry once with a one-shot policy adjustment (eg. plain reasoning).
    RetryOnceWithAdjustment,
    /// Don't retry — propagate.
    Abort,
}

impl NormalizedError {
    /// Choose what to do given the current attempt count (1-indexed).
    /// `attempt = 1` is the first try, `attempt = 2` would be the retry.
    #[must_use]
    pub fn classify(&self, attempt: u32) -> RetryDecision {
        match self {
            Self::Auth { .. }
            | Self::Permanent { .. }
            | Self::ContextOverflow { .. }
            | Self::UnknownModel { .. } => RetryDecision::Abort,
            Self::RateLimit { .. } | Self::Transient { .. } => {
                if attempt < 3 {
                    RetryDecision::RetryOnce
                } else {
                    RetryDecision::Abort
                }
            }
            Self::InvalidOpaqueReasoning { .. } => {
                if attempt == 1 {
                    RetryDecision::RetryOnceWithAdjustment
                } else {
                    RetryDecision::Abort
                }
            }
        }
    }

    /// Stable short tag the daemon writes into events for grouping.
    #[must_use]
    pub fn tag(&self) -> &'static str {
        match self {
            Self::Auth { .. } => "auth",
            Self::RateLimit { .. } => "rate_limit",
            Self::InvalidOpaqueReasoning { .. } => "invalid_opaque_reasoning",
            Self::Transient { .. } => "transient",
            Self::Permanent { .. } => "permanent",
            Self::ContextOverflow { .. } => "context_overflow",
            Self::UnknownModel { .. } => "unknown_model",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auth_never_retries() {
        let e = NormalizedError::Auth {
            message: "bad key".into(),
        };
        for attempt in 1..5 {
            assert_eq!(e.classify(attempt), RetryDecision::Abort);
        }
    }

    #[test]
    fn rate_limit_retries_then_aborts() {
        let e = NormalizedError::RateLimit {
            retry_after_ms: 1000,
            message: "slow down".into(),
        };
        assert_eq!(e.classify(1), RetryDecision::RetryOnce);
        assert_eq!(e.classify(2), RetryDecision::RetryOnce);
        assert_eq!(e.classify(3), RetryDecision::Abort);
    }

    #[test]
    fn invalid_opaque_reasoning_one_shot_adjustment_then_abort() {
        let e = NormalizedError::InvalidOpaqueReasoning {
            message: "summary missing".into(),
        };
        assert_eq!(e.classify(1), RetryDecision::RetryOnceWithAdjustment);
        assert_eq!(e.classify(2), RetryDecision::Abort);
    }

    #[test]
    fn permanent_aborts() {
        let e = NormalizedError::Permanent {
            message: "service gone".into(),
        };
        assert_eq!(e.classify(1), RetryDecision::Abort);
    }

    #[test]
    fn context_overflow_aborts_to_force_compaction() {
        let e = NormalizedError::ContextOverflow {
            message: "200k > 128k".into(),
        };
        assert_eq!(e.classify(1), RetryDecision::Abort);
    }

    #[test]
    fn unknown_model_aborts() {
        let e = NormalizedError::UnknownModel {
            model: "o1-deprecated".into(),
            message: "model retired".into(),
        };
        assert_eq!(e.classify(1), RetryDecision::Abort);
    }

    #[test]
    fn tags_are_unique_and_stable() {
        let cases = [
            NormalizedError::Auth {
                message: "x".into(),
            },
            NormalizedError::RateLimit {
                retry_after_ms: 0,
                message: "x".into(),
            },
            NormalizedError::InvalidOpaqueReasoning {
                message: "x".into(),
            },
            NormalizedError::Transient {
                message: "x".into(),
            },
            NormalizedError::Permanent {
                message: "x".into(),
            },
            NormalizedError::ContextOverflow {
                message: "x".into(),
            },
            NormalizedError::UnknownModel {
                model: "m".into(),
                message: "x".into(),
            },
        ];
        let mut seen = std::collections::HashSet::new();
        for e in &cases {
            assert!(seen.insert(e.tag()), "duplicate tag {}", e.tag());
        }
    }

    #[test]
    fn serde_round_trips_with_kind_tag() {
        let e = NormalizedError::RateLimit {
            retry_after_ms: 500,
            message: "slow".into(),
        };
        let s = serde_json::to_string(&e).unwrap();
        assert!(s.contains("\"kind\":\"rate_limit\""));
        let back: NormalizedError = serde_json::from_str(&s).unwrap();
        assert_eq!(back, e);
    }
}
