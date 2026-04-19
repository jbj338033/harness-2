use crate::{Result, SessionError};
use harness_core::{AgentId, SessionId, now};
use harness_storage::WriterHandle;
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentStatus {
    Pending,
    Running,
    Done,
    Failed,
}

impl AgentStatus {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            AgentStatus::Pending => "pending",
            AgentStatus::Running => "running",
            AgentStatus::Done => "done",
            AgentStatus::Failed => "failed",
        }
    }

    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "pending" => Ok(Self::Pending),
            "running" => Ok(Self::Running),
            "done" => Ok(Self::Done),
            "failed" => Ok(Self::Failed),
            other => Err(SessionError::InvalidState(format!(
                "unknown status {other}"
            ))),
        }
    }

    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, AgentStatus::Done | AgentStatus::Failed)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentRecord {
    pub id: AgentId,
    pub session_id: SessionId,
    pub parent_id: Option<AgentId>,
    pub role: String,
    pub model: String,
    pub status: AgentStatus,
    pub system_prompt: Option<String>,
    pub worktree_path: Option<String>,
    pub wave: Option<i64>,
    pub iteration: i64,
    pub created_at: i64,
    pub completed_at: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct NewAgent {
    pub session_id: SessionId,
    pub parent_id: Option<AgentId>,
    pub role: String,
    pub model: String,
    pub system_prompt: Option<String>,
    pub worktree_path: Option<String>,
    pub wave: Option<i64>,
}

pub async fn insert(writer: &WriterHandle, new: NewAgent) -> Result<AgentId> {
    let id = AgentId::new();
    let ts = now().as_millis();
    let id_s = id.as_uuid().to_string();
    let sid_s = new.session_id.as_uuid().to_string();
    let pid_s = new.parent_id.map(|p| p.as_uuid().to_string());
    writer
        .execute(move |c| {
            c.execute(
                "INSERT INTO agents (
                    id, session_id, parent_id, role, model, status,
                    system_prompt, worktree_path, wave, iteration,
                    created_at, completed_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, 'pending', ?6, ?7, ?8, 1, ?9, NULL)",
                params![
                    id_s,
                    sid_s,
                    pid_s,
                    new.role,
                    new.model,
                    new.system_prompt,
                    new.worktree_path,
                    new.wave,
                    ts,
                ],
            )?;
            Ok(())
        })
        .await?;
    Ok(id)
}

pub async fn set_status(writer: &WriterHandle, id: AgentId, status: AgentStatus) -> Result<()> {
    let id_s = id.as_uuid().to_string();
    let s = status.as_str();
    let ts = now().as_millis();
    let completed = status.is_terminal().then_some(ts);
    writer
        .execute(move |c| {
            let rows = c.execute(
                "UPDATE agents SET status = ?1, completed_at = COALESCE(?2, completed_at) WHERE id = ?3",
                params![s, completed, id_s],
            )?;
            if rows == 0 {
                return Err(harness_storage::StorageError::NotFound(format!(
                    "agent {id_s}"
                )));
            }
            Ok(())
        })
        .await?;
    Ok(())
}

pub async fn bump_iteration(writer: &WriterHandle, id: AgentId) -> Result<()> {
    let id_s = id.as_uuid().to_string();
    writer
        .execute(move |c| {
            c.execute(
                "UPDATE agents SET iteration = iteration + 1 WHERE id = ?1",
                params![id_s],
            )?;
            Ok(())
        })
        .await?;
    Ok(())
}

pub fn get(conn: &Connection, id: AgentId) -> Result<AgentRecord> {
    let id_s = id.as_uuid().to_string();
    conn.query_row(
        "SELECT id, session_id, parent_id, role, model, status, system_prompt,
                worktree_path, wave, iteration, created_at, completed_at
         FROM agents WHERE id = ?1",
        params![id_s],
        row_to_agent,
    )
    .map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => SessionError::NotFound(format!("agent {id_s}")),
        other => SessionError::Storage(harness_storage::StorageError::Sqlite(other)),
    })
}

pub fn list_for_session(conn: &Connection, session: SessionId) -> Result<Vec<AgentRecord>> {
    let sid = session.as_uuid().to_string();
    let mut stmt = conn
        .prepare(
            "SELECT id, session_id, parent_id, role, model, status, system_prompt,
                    worktree_path, wave, iteration, created_at, completed_at
             FROM agents WHERE session_id = ?1 ORDER BY created_at ASC",
        )
        .map_err(|e| SessionError::Storage(harness_storage::StorageError::Sqlite(e)))?;
    let iter = stmt
        .query_map(params![sid], row_to_agent)
        .map_err(|e| SessionError::Storage(harness_storage::StorageError::Sqlite(e)))?;
    let mut out = Vec::new();
    for r in iter {
        out.push(r.map_err(|e| SessionError::Storage(harness_storage::StorageError::Sqlite(e)))?);
    }
    Ok(out)
}

pub fn find_stale_running(conn: &Connection) -> Result<Vec<AgentRecord>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, session_id, parent_id, role, model, status, system_prompt,
                    worktree_path, wave, iteration, created_at, completed_at
             FROM agents WHERE status = 'running'",
        )
        .map_err(|e| SessionError::Storage(harness_storage::StorageError::Sqlite(e)))?;
    let iter = stmt
        .query_map([], row_to_agent)
        .map_err(|e| SessionError::Storage(harness_storage::StorageError::Sqlite(e)))?;
    let mut out = Vec::new();
    for r in iter {
        out.push(r.map_err(|e| SessionError::Storage(harness_storage::StorageError::Sqlite(e)))?);
    }
    Ok(out)
}

fn row_to_agent(row: &rusqlite::Row) -> rusqlite::Result<AgentRecord> {
    let id_s: String = row.get(0)?;
    let sid_s: String = row.get(1)?;
    let pid_s: Option<String> = row.get(2)?;
    let status_s: String = row.get(5)?;
    let to_uuid = |s: &str| {
        uuid::Uuid::parse_str(s).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
        })
    };
    Ok(AgentRecord {
        id: AgentId::from(to_uuid(&id_s)?),
        session_id: SessionId::from(to_uuid(&sid_s)?),
        parent_id: pid_s
            .as_deref()
            .map(to_uuid)
            .transpose()?
            .map(AgentId::from),
        role: row.get(3)?,
        model: row.get(4)?,
        status: AgentStatus::parse(&status_s).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(
                5,
                rusqlite::types::Type::Text,
                Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    e.to_string(),
                )),
            )
        })?,
        system_prompt: row.get(6)?,
        worktree_path: row.get(7)?,
        wave: row.get(8)?,
        iteration: row.get(9)?,
        created_at: row.get(10)?,
        completed_at: row.get(11)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manager::SessionManager;
    use harness_storage::{Database, Writer};
    use tempfile::NamedTempFile;

    async fn setup() -> (NamedTempFile, WriterHandle, SessionId) {
        let f = NamedTempFile::new().unwrap();
        Database::open(f.path()).unwrap();
        let h = Writer::spawn(f.path()).unwrap();
        let mgr = SessionManager::new(&h);
        let s = mgr.create("/tmp", None).await.unwrap();
        (f, h, s.id)
    }

    #[test]
    fn status_roundtrip() {
        for s in [
            AgentStatus::Pending,
            AgentStatus::Running,
            AgentStatus::Done,
            AgentStatus::Failed,
        ] {
            assert_eq!(AgentStatus::parse(s.as_str()).unwrap(), s);
        }
    }

    #[test]
    fn terminal_predicate() {
        assert!(!AgentStatus::Pending.is_terminal());
        assert!(!AgentStatus::Running.is_terminal());
        assert!(AgentStatus::Done.is_terminal());
        assert!(AgentStatus::Failed.is_terminal());
    }

    #[tokio::test]
    async fn insert_and_transitions() {
        let (f, w, sid) = setup().await;
        let id = insert(
            &w,
            NewAgent {
                session_id: sid,
                parent_id: None,
                role: "root".into(),
                model: "claude-sonnet-4-6".into(),
                system_prompt: None,
                worktree_path: None,
                wave: None,
            },
        )
        .await
        .unwrap();

        set_status(&w, id, AgentStatus::Running).await.unwrap();
        set_status(&w, id, AgentStatus::Done).await.unwrap();

        let reader = rusqlite::Connection::open(f.path()).unwrap();
        let got = get(&reader, id).unwrap();
        assert_eq!(got.status, AgentStatus::Done);
        assert!(got.completed_at.is_some());
        assert_eq!(got.iteration, 1);
    }

    #[tokio::test]
    async fn iteration_bump() {
        let (f, w, sid) = setup().await;
        let id = insert(
            &w,
            NewAgent {
                session_id: sid,
                parent_id: None,
                role: "root".into(),
                model: "m".into(),
                system_prompt: None,
                worktree_path: None,
                wave: None,
            },
        )
        .await
        .unwrap();
        bump_iteration(&w, id).await.unwrap();
        bump_iteration(&w, id).await.unwrap();
        let reader = rusqlite::Connection::open(f.path()).unwrap();
        assert_eq!(get(&reader, id).unwrap().iteration, 3);
    }

    #[tokio::test]
    async fn list_and_find_stale() {
        let (f, w, sid) = setup().await;
        for role in ["root", "coder", "reviewer"] {
            insert(
                &w,
                NewAgent {
                    session_id: sid,
                    parent_id: None,
                    role: role.into(),
                    model: "m".into(),
                    system_prompt: None,
                    worktree_path: None,
                    wave: None,
                },
            )
            .await
            .unwrap();
        }
        let reader = rusqlite::Connection::open(f.path()).unwrap();
        let agents = list_for_session(&reader, sid).unwrap();
        assert_eq!(agents.len(), 3);

        set_status(&w, agents[0].id, AgentStatus::Running)
            .await
            .unwrap();

        let reader = rusqlite::Connection::open(f.path()).unwrap();
        let stale = find_stale_running(&reader).unwrap();
        assert_eq!(stale.len(), 1);
        assert_eq!(stale[0].id, agents[0].id);
    }
}
