use crate::{Result, SessionError};
use harness_core::{SessionId, now};
use harness_storage::WriterHandle;
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionRecord {
    pub id: SessionId,
    pub title: Option<String>,
    pub cwd: String,
    pub task: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

pub struct SessionManager<'a> {
    writer: &'a WriterHandle,
}

impl<'a> SessionManager<'a> {
    #[must_use]
    pub fn new(writer: &'a WriterHandle) -> Self {
        Self { writer }
    }

    pub async fn create(
        &self,
        cwd: impl Into<String>,
        task: Option<String>,
    ) -> Result<SessionRecord> {
        let id = SessionId::new();
        let cwd = cwd.into();
        let ts = now().as_millis();
        let id_s = id.as_uuid().to_string();
        let cwd_ = cwd.clone();
        let task_ = task.clone();

        self.writer
            .execute(move |conn| {
                conn.execute(
                    "INSERT INTO sessions (id, title, cwd, task, created_at, updated_at)
                     VALUES (?1, NULL, ?2, ?3, ?4, ?4)",
                    params![id_s, cwd_, task_, ts],
                )?;
                Ok(())
            })
            .await?;

        Ok(SessionRecord {
            id,
            title: None,
            cwd,
            task,
            created_at: ts,
            updated_at: ts,
        })
    }

    pub async fn set_title(&self, id: SessionId, title: impl Into<String>) -> Result<()> {
        let title = title.into();
        let id_s = id.as_uuid().to_string();
        let ts = now().as_millis();
        self.writer
            .execute(move |conn| {
                let affected = conn.execute(
                    "UPDATE sessions SET title = ?1, updated_at = ?2 WHERE id = ?3",
                    params![title, ts, id_s],
                )?;
                if affected == 0 {
                    return Err(harness_storage::StorageError::NotFound(format!(
                        "session {id_s}"
                    )));
                }
                Ok(())
            })
            .await?;
        Ok(())
    }

    pub async fn touch(&self, id: SessionId) -> Result<()> {
        let id_s = id.as_uuid().to_string();
        let ts = now().as_millis();
        self.writer
            .execute(move |conn| {
                conn.execute(
                    "UPDATE sessions SET updated_at = ?1 WHERE id = ?2",
                    params![ts, id_s],
                )?;
                Ok(())
            })
            .await?;
        Ok(())
    }

    pub async fn delete(&self, id: SessionId) -> Result<()> {
        let id_s = id.as_uuid().to_string();
        self.writer
            .execute(move |conn| {
                conn.execute("DELETE FROM sessions WHERE id = ?1", params![id_s])?;
                Ok(())
            })
            .await?;
        Ok(())
    }

    pub fn get(conn: &Connection, id: SessionId) -> Result<SessionRecord> {
        let id_s = id.as_uuid().to_string();
        let rec = conn
            .query_row(
                "SELECT id, title, cwd, task, created_at, updated_at FROM sessions WHERE id = ?1",
                params![id_s],
                row_to_session,
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => {
                    SessionError::NotFound(format!("session {id_s}"))
                }
                other => SessionError::Storage(harness_storage::StorageError::Sqlite(other)),
            })?;
        Ok(rec)
    }

    pub fn list(conn: &Connection, limit: usize) -> Result<Vec<SessionRecord>> {
        let mut stmt = conn
            .prepare(
                "SELECT id, title, cwd, task, created_at, updated_at
                 FROM sessions
                 ORDER BY updated_at DESC
                 LIMIT ?1",
            )
            .map_err(|e| SessionError::Storage(harness_storage::StorageError::Sqlite(e)))?;
        let iter = stmt
            .query_map(
                params![i64::try_from(limit).unwrap_or(i64::MAX)],
                row_to_session,
            )
            .map_err(|e| SessionError::Storage(harness_storage::StorageError::Sqlite(e)))?;
        let mut out = Vec::new();
        for r in iter {
            out.push(
                r.map_err(|e| SessionError::Storage(harness_storage::StorageError::Sqlite(e)))?,
            );
        }
        Ok(out)
    }
}

fn row_to_session(row: &rusqlite::Row) -> rusqlite::Result<SessionRecord> {
    let id_s: String = row.get(0)?;
    let id = uuid::Uuid::parse_str(&id_s).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
    })?;
    Ok(SessionRecord {
        id: SessionId::from(id),
        title: row.get(1)?,
        cwd: row.get(2)?,
        task: row.get(3)?,
        created_at: row.get(4)?,
        updated_at: row.get(5)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use harness_storage::{Database, Writer};
    use tempfile::NamedTempFile;

    fn setup() -> (NamedTempFile, WriterHandle) {
        let f = NamedTempFile::new().unwrap();
        Database::open(f.path()).unwrap();
        let h = Writer::spawn(f.path()).unwrap();
        (f, h)
    }

    #[tokio::test]
    async fn create_list_get() {
        let (f, w) = setup();
        let mgr = SessionManager::new(&w);
        let s = mgr
            .create("/tmp/proj", Some("fix bug".into()))
            .await
            .unwrap();

        let reader = rusqlite::Connection::open(f.path()).unwrap();
        let got = SessionManager::get(&reader, s.id).unwrap();
        assert_eq!(got.cwd, "/tmp/proj");
        assert_eq!(got.task.as_deref(), Some("fix bug"));

        let all = SessionManager::list(&reader, 10).unwrap();
        assert_eq!(all.len(), 1);
    }

    #[tokio::test]
    async fn set_title_persists() {
        let (f, w) = setup();
        let mgr = SessionManager::new(&w);
        let s = mgr.create("/tmp", None).await.unwrap();
        mgr.set_title(s.id, "my title").await.unwrap();

        let reader = rusqlite::Connection::open(f.path()).unwrap();
        let got = SessionManager::get(&reader, s.id).unwrap();
        assert_eq!(got.title.as_deref(), Some("my title"));
    }

    #[tokio::test]
    async fn delete_cascades_children() {
        use rusqlite::params;
        let (f, w) = setup();
        let mgr = SessionManager::new(&w);
        let s = mgr.create("/tmp", None).await.unwrap();

        let sid = s.id.as_uuid().to_string();
        w.execute(move |c| {
            c.execute(
                "INSERT INTO agents (id, session_id, role, model, status, created_at)
                 VALUES ('a', ?1, 'root', 'm', 'pending', 0)",
                params![sid],
            )?;
            Ok(())
        })
        .await
        .unwrap();

        mgr.delete(s.id).await.unwrap();

        let reader = rusqlite::Connection::open(f.path()).unwrap();
        let sess_count: i64 = reader
            .query_row("SELECT COUNT(*) FROM sessions", [], |r| r.get(0))
            .unwrap();
        let ag_count: i64 = reader
            .query_row("SELECT COUNT(*) FROM agents", [], |r| r.get(0))
            .unwrap();
        assert_eq!(sess_count, 0);
        assert_eq!(ag_count, 0, "agents should cascade-delete with session");
    }

    #[tokio::test]
    async fn list_orders_by_updated_at_desc() {
        let (f, w) = setup();
        let mgr = SessionManager::new(&w);
        let a = mgr.create("/a", None).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        let b = mgr.create("/b", None).await.unwrap();

        let reader = rusqlite::Connection::open(f.path()).unwrap();
        let sessions = SessionManager::list(&reader, 10).unwrap();
        assert_eq!(sessions[0].id, b.id);
        assert_eq!(sessions[1].id, a.id);
    }

    #[tokio::test]
    async fn get_unknown_returns_not_found() {
        let (f, _w) = setup();
        let reader = rusqlite::Connection::open(f.path()).unwrap();
        let err = SessionManager::get(&reader, SessionId::new()).unwrap_err();
        assert!(matches!(err, SessionError::NotFound(_)));
    }
}
