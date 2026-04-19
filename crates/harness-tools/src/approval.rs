use harness_core::SessionId;
use harness_storage::{
    WriterHandle,
    approvals::{self, Scope},
};
use rusqlite::Connection;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalDecision {
    Once,
    AlwaysSession,
    AlwaysGlobal,
    Deny,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalOutcome {
    Allowed,
    Granted(ApprovalDecision),
    Denied,
}

pub struct ApprovalGate<'a> {
    pub writer: &'a WriterHandle,
}

impl ApprovalGate<'_> {
    pub fn is_preallowed(
        conn: &Connection,
        session: Option<SessionId>,
        command: &str,
    ) -> harness_storage::Result<bool> {
        approvals::matches(conn, session, command)
    }

    pub async fn grant(
        &self,
        session: Option<SessionId>,
        pattern: impl Into<String>,
        scope: Scope,
    ) -> harness_storage::Result<String> {
        approvals::insert(self.writer, session, pattern, scope, None).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use harness_storage::{Database, Writer};
    use tempfile::NamedTempFile;

    fn setup() -> (NamedTempFile, WriterHandle) {
        let f = NamedTempFile::new().unwrap();
        let _db = Database::open(f.path()).unwrap();
        let h = Writer::spawn(f.path()).unwrap();
        (f, h)
    }

    #[tokio::test]
    async fn preallowed_from_global_scope() {
        let (f, writer) = setup();
        let gate = ApprovalGate { writer: &writer };
        gate.grant(None, "cargo test", Scope::Global).await.unwrap();

        let reader = Connection::open(f.path()).unwrap();
        assert!(ApprovalGate::is_preallowed(&reader, None, "cargo test --release").unwrap());
        assert!(!ApprovalGate::is_preallowed(&reader, None, "rm -rf /").unwrap());
    }

    #[tokio::test]
    async fn session_scoped_doesnt_leak() {
        let (f, writer) = setup();
        let gate = ApprovalGate { writer: &writer };
        let s = SessionId::new();
        gate.grant(Some(s), "npm install", Scope::Session)
            .await
            .unwrap();

        let reader = Connection::open(f.path()).unwrap();
        assert!(ApprovalGate::is_preallowed(&reader, Some(s), "npm install pkg").unwrap());
        assert!(
            !ApprovalGate::is_preallowed(&reader, Some(SessionId::new()), "npm install pkg")
                .unwrap()
        );
    }
}
