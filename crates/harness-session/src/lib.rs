pub mod agent;
pub mod broadcast;
pub mod manager;
pub mod message;
pub mod tool_call;

pub use agent::{AgentRecord, AgentStatus};
pub use broadcast::{SessionBroadcaster, SessionEvent};
pub use manager::{SessionManager, SessionRecord};
pub use message::{MessageRecord, MessageRole};
pub use tool_call::ToolCallRecord;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum SessionError {
    #[error("storage: {0}")]
    Storage(#[from] harness_storage::StorageError),

    #[error("not found: {0}")]
    NotFound(String),

    #[error("invalid state: {0}")]
    InvalidState(String),

    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, SessionError>;
