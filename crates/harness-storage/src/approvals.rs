use crate::{Result, WriterHandle};
use harness_core::{SessionId, now};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Scope {
    Session,
    Global,
}

impl Scope {
    #[must_use]
    fn as_str(self) -> &'static str {
        match self {
            Scope::Session => "session",
            Scope::Global => "global",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Approval {
    pub id: String,
    pub session_id: Option<SessionId>,
    pub pattern: String,
    pub scope: Scope,
    pub expires_at: Option<i64>,
    pub created_at: i64,
}

pub async fn insert(
    writer: &WriterHandle,
    session_id: Option<SessionId>,
    pattern: impl Into<String>,
    scope: Scope,
    expires_at: Option<i64>,
) -> Result<String> {
    let pattern = pattern.into();
    let id = uuid::Uuid::new_v4().to_string();
    let created_at = now().as_millis();

    let session_id_str = session_id.map(|s| s.as_uuid().to_string());
    let scope_str = scope.as_str();
    let id_clone = id.clone();

    writer
        .execute(move |conn| {
            conn.execute(
                "INSERT INTO approvals (id, session_id, pattern, scope, expires_at, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    id_clone,
                    session_id_str,
                    pattern,
                    scope_str,
                    expires_at,
                    created_at
                ],
            )?;
            Ok(())
        })
        .await?;

    Ok(id)
}

pub fn matches(conn: &Connection, session: Option<SessionId>, command: &str) -> Result<bool> {
    let now_ms = now().as_millis();
    let session_str = session.map(|s| s.as_uuid().to_string());

    let mut stmt = conn.prepare(
        "SELECT pattern FROM approvals
         WHERE (expires_at IS NULL OR expires_at > ?1)
           AND (scope = 'global' OR session_id = ?2)",
    )?;

    let mut rows = stmt.query(params![now_ms, session_str])?;
    while let Some(row) = rows.next()? {
        let pattern: String = row.get(0)?;
        if command.contains(&pattern) {
            return Ok(true);
        }
    }
    Ok(false)
}

pub async fn revoke(writer: &WriterHandle, id: String) -> Result<bool> {
    writer
        .execute(move |conn| {
            let affected = conn.execute("DELETE FROM approvals WHERE id = ?1", params![id])?;
            Ok(affected > 0)
        })
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Writer, db, migrations};
    use tempfile::NamedTempFile;

    fn setup() -> (NamedTempFile, WriterHandle) {
        let f = NamedTempFile::new().unwrap();
        let mut conn = Connection::open(f.path()).unwrap();
        db::configure(&mut conn).unwrap();
        migrations::apply(&mut conn).unwrap();
        drop(conn);
        let h = Writer::spawn(f.path()).unwrap();
        (f, h)
    }

    #[tokio::test]
    async fn global_approval_matches_any_session() {
        let (f, h) = setup();
        insert(&h, None, "cargo test", Scope::Global, None)
            .await
            .unwrap();

        let reader = Connection::open(f.path()).unwrap();
        assert!(matches(&reader, Some(SessionId::new()), "cargo test --release").unwrap());
        assert!(matches(&reader, None, "cargo test").unwrap());
        assert!(!matches(&reader, None, "rm -rf /").unwrap());
    }

    #[tokio::test]
    async fn session_approval_scoped() {
        let (f, h) = setup();
        let s = SessionId::new();
        insert(&h, Some(s), "npm install", Scope::Session, None)
            .await
            .unwrap();

        let reader = Connection::open(f.path()).unwrap();
        assert!(matches(&reader, Some(s), "npm install foo").unwrap());
        assert!(!matches(&reader, Some(SessionId::new()), "npm install foo").unwrap());
    }

    #[tokio::test]
    async fn expired_approval_does_not_match() {
        let (f, h) = setup();
        let past = now().as_millis() - 10_000;
        insert(&h, None, "old cmd", Scope::Global, Some(past))
            .await
            .unwrap();

        let reader = Connection::open(f.path()).unwrap();
        assert!(!matches(&reader, None, "old cmd").unwrap());
    }

    #[tokio::test]
    async fn revoke_removes_approval() {
        let (f, h) = setup();
        let id = insert(&h, None, "ls", Scope::Global, None).await.unwrap();
        let removed = revoke(&h, id).await.unwrap();
        assert!(removed);

        let reader = Connection::open(f.path()).unwrap();
        assert!(!matches(&reader, None, "ls -la").unwrap());
    }
}
