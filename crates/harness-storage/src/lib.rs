pub mod approvals;
pub mod backup;
pub mod config;
pub mod credentials;
pub mod db;
pub mod events;
pub mod migrations;
pub mod reader;
pub mod reader_pool;
pub mod workspace_trust;
pub mod writer;

pub use db::{Database, open, open_in_memory};
pub use reader::Reader;
pub use reader_pool::{DEFAULT_MAX_IDLE, PooledReader, ReaderPool};
pub use writer::{Writer, WriterHandle};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("migration: {0}")]
    Migration(#[from] rusqlite_migration::Error),

    #[error("json: {0}")]
    Json(#[from] serde_json::Error),

    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    #[error("writer task is unavailable (shut down)")]
    WriterUnavailable,

    #[error("not found: {0}")]
    NotFound(String),

    #[error("invariant: {0}")]
    Invariant(String),
}

pub type Result<T> = std::result::Result<T, StorageError>;

impl From<StorageError> for harness_core::HarnessError {
    fn from(e: StorageError) -> Self {
        match e {
            StorageError::NotFound(msg) => Self::NotFound(msg),
            StorageError::Io(e) => Self::Io(e),
            other => Self::Database(other.to_string()),
        }
    }
}
