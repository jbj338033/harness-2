use async_trait::async_trait;
use harness_core::{AgentId, SessionId};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::Arc;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalVerdict {
    Allowed,
    Denied,
}

#[async_trait]
pub trait ApprovalRequester: std::fmt::Debug + Send + Sync {
    async fn request(
        &self,
        session: SessionId,
        command: String,
        pattern: String,
        reason: String,
    ) -> ApprovalVerdict;
}

#[derive(Debug, Clone)]
pub struct ToolContext {
    pub session: SessionId,
    pub agent: AgentId,
    pub cwd: PathBuf,
    pub allowed_writes: Option<Vec<PathBuf>>,
    pub is_root: bool,
    pub approval: Option<Arc<dyn ApprovalRequester>>,
}

impl ToolContext {
    #[must_use]
    pub fn test(cwd: impl Into<PathBuf>) -> Self {
        Self {
            session: SessionId::new(),
            agent: AgentId::new(),
            cwd: cwd.into(),
            allowed_writes: None,
            is_root: true,
            approval: None,
        }
    }

    #[must_use]
    pub fn can_write(&self, path: &std::path::Path) -> bool {
        if self.is_root {
            return true;
        }
        let Some(list) = &self.allowed_writes else {
            return true;
        };
        let abs = if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.cwd.join(path)
        };
        list.iter().any(|allowed| {
            let allowed_abs = if allowed.is_absolute() {
                allowed.clone()
            } else {
                self.cwd.join(allowed)
            };
            abs.starts_with(&allowed_abs) || abs == allowed_abs
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolOutput {
    pub content: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub is_error: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
}

impl ToolOutput {
    pub fn ok(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            is_error: false,
            metadata: None,
        }
    }

    pub fn err(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            is_error: true,
            metadata: None,
        }
    }

    #[must_use]
    pub fn with_metadata(mut self, meta: Value) -> Self {
        self.metadata = Some(meta);
        self
    }
}

#[derive(Debug, Error)]
pub enum ToolError {
    #[error("invalid input: {0}")]
    Input(String),
    #[error("denied: {0}")]
    Denied(String),
    #[error("out of scope: {path}")]
    OutOfScope { path: String },
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Other(String),
}

#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;

    fn description(&self) -> &str;

    fn input_schema(&self) -> Value;

    async fn execute(&self, input: Value, ctx: &ToolContext) -> Result<ToolOutput, ToolError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use tempfile::TempDir;

    #[test]
    fn root_can_write_anywhere() {
        let tmp = TempDir::new().unwrap();
        let ctx = ToolContext {
            session: SessionId::new(),
            agent: AgentId::new(),
            cwd: tmp.path().to_path_buf(),
            allowed_writes: Some(vec![PathBuf::from("only.rs")]),
            is_root: true,
            approval: None,
        };
        assert!(ctx.can_write(Path::new("anything.rs")));
        assert!(ctx.can_write(Path::new("/etc/hosts")));
    }

    #[test]
    fn worker_restricted_to_allowlist() {
        let tmp = TempDir::new().unwrap();
        let ctx = ToolContext {
            session: SessionId::new(),
            agent: AgentId::new(),
            cwd: tmp.path().to_path_buf(),
            allowed_writes: Some(vec![PathBuf::from("src/lib.rs")]),
            is_root: false,
            approval: None,
        };
        assert!(ctx.can_write(Path::new("src/lib.rs")));
        assert!(!ctx.can_write(Path::new("src/main.rs")));
        assert!(!ctx.can_write(Path::new("/etc/hosts")));
    }

    #[test]
    fn worker_with_no_allowlist_is_unrestricted() {
        let ctx = ToolContext {
            session: SessionId::new(),
            agent: AgentId::new(),
            cwd: PathBuf::from("/tmp"),
            allowed_writes: None,
            is_root: false,
            approval: None,
        };
        assert!(ctx.can_write(Path::new("anywhere.rs")));
    }

    #[test]
    fn tool_output_roundtrips() {
        let out = ToolOutput::ok("hello").with_metadata(serde_json::json!({"n": 1}));
        let s = serde_json::to_string(&out).unwrap();
        let back: ToolOutput = serde_json::from_str(&s).unwrap();
        assert_eq!(back.content, "hello");
        assert!(!back.is_error);
        assert_eq!(back.metadata.unwrap()["n"], 1);
    }
}
