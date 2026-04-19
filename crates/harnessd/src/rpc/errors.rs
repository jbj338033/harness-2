use harness_proto::{ErrorCode, ErrorObject};
use harness_session::SessionError;
use harness_storage::StorageError;

pub fn rpc_err(code: ErrorCode, e: impl std::fmt::Display) -> ErrorObject {
    ErrorObject::new(code, e.to_string())
}

pub fn map_storage_err(e: StorageError) -> ErrorObject {
    match e {
        StorageError::NotFound(msg) => rpc_err(ErrorCode::NotFound, msg),
        StorageError::Invariant(msg) => rpc_err(ErrorCode::InvalidParams, msg),
        other => rpc_err(ErrorCode::InternalError, other),
    }
}

pub fn map_session_err(e: SessionError) -> ErrorObject {
    match e {
        SessionError::NotFound(msg) => rpc_err(ErrorCode::NotFound, msg),
        SessionError::InvalidState(msg) => rpc_err(ErrorCode::InvalidParams, msg),
        SessionError::Storage(s) => map_storage_err(s),
        SessionError::Other(msg) => rpc_err(ErrorCode::InternalError, msg),
    }
}
