pub mod id;
pub mod principles;
pub mod time;

pub use id::{AgentId, MessageId, SessionId, ToolCallId};
pub use principles::Principle;
pub use time::{Timestamp, now};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum HarnessError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(String),

    #[error("database error: {0}")]
    Database(String),

    #[error("not found: {0}")]
    NotFound(String),

    #[error("permission denied: {0}")]
    PermissionDenied(String),

    #[error("principle violation ({principle}): {reason}")]
    PrincipleViolation {
        principle: Principle,
        reason: String,
    },

    #[error("operation cancelled")]
    Cancelled,

    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, HarnessError>;
