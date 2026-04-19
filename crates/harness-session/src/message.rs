use crate::{Result, SessionError};
use harness_core::{AgentId, MessageId, now};
use harness_storage::WriterHandle;
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    User,
    Assistant,
    System,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageKind {
    #[default]
    Chat,
    SkillAttachment,
}

impl MessageKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            MessageKind::Chat => "chat",
            MessageKind::SkillAttachment => "skill_attachment",
        }
    }

    #[must_use]
    pub fn parse_or_chat(s: &str) -> Self {
        match s {
            "skill_attachment" => Self::SkillAttachment,
            _ => Self::Chat,
        }
    }
}

impl MessageRole {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            MessageRole::User => "user",
            MessageRole::Assistant => "assistant",
            MessageRole::System => "system",
        }
    }

    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "user" => Ok(Self::User),
            "assistant" => Ok(Self::Assistant),
            "system" => Ok(Self::System),
            other => Err(SessionError::InvalidState(format!("unknown role {other}"))),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MessageRecord {
    pub id: MessageId,
    pub agent_id: AgentId,
    pub role: MessageRole,
    pub content: Option<String>,
    pub tokens_in: Option<i64>,
    pub tokens_out: Option<i64>,
    pub cost: Option<f64>,
    pub model: Option<String>,
    pub kind: MessageKind,
    pub created_at: i64,
}

#[derive(Debug, Clone)]
pub struct NewMessage {
    pub agent_id: AgentId,
    pub role: MessageRole,
    pub content: Option<String>,
    pub model: Option<String>,
    pub kind: MessageKind,
}

pub async fn insert(writer: &WriterHandle, msg: NewMessage) -> Result<(MessageId, i64)> {
    let id = MessageId::new();
    let ts = now().as_millis();
    let id_s = id.as_uuid().to_string();
    let aid_s = msg.agent_id.as_uuid().to_string();
    let role = msg.role.as_str();
    let content = msg.content.clone();
    let model = msg.model.clone();
    let kind = msg.kind.as_str();
    writer
        .execute(move |c| {
            c.execute(
                "INSERT INTO messages (id, agent_id, role, content, model, kind, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![id_s, aid_s, role, content, model, kind, ts],
            )?;
            Ok(())
        })
        .await?;
    Ok((id, ts))
}

pub async fn append_content(
    writer: &WriterHandle,
    id: MessageId,
    append: String,
    tokens_in: Option<i64>,
    tokens_out: Option<i64>,
    cost: Option<f64>,
) -> Result<()> {
    let id_s = id.as_uuid().to_string();
    writer
        .execute(move |c| {
            c.execute(
                "UPDATE messages
                   SET content = COALESCE(content, '') || ?1,
                       tokens_in = COALESCE(?2, tokens_in),
                       tokens_out = COALESCE(?3, tokens_out),
                       cost = COALESCE(?4, cost)
                 WHERE id = ?5",
                params![append, tokens_in, tokens_out, cost, id_s],
            )?;
            Ok(())
        })
        .await?;
    Ok(())
}

pub fn list_for_agent(conn: &Connection, agent: AgentId) -> Result<Vec<MessageRecord>> {
    let aid = agent.as_uuid().to_string();
    let mut stmt = conn
        .prepare(
            "SELECT id, agent_id, role, content, tokens_in, tokens_out, cost, model, kind, created_at
             FROM messages WHERE agent_id = ?1 ORDER BY created_at ASC, rowid ASC",
        )
        .map_err(|e| SessionError::Storage(harness_storage::StorageError::Sqlite(e)))?;
    let iter = stmt
        .query_map(params![aid], row_to_message)
        .map_err(|e| SessionError::Storage(harness_storage::StorageError::Sqlite(e)))?;
    let mut out = Vec::new();
    for r in iter {
        out.push(r.map_err(|e| SessionError::Storage(harness_storage::StorageError::Sqlite(e)))?);
    }
    Ok(out)
}

pub fn search(conn: &Connection, query: &str, limit: usize) -> Result<Vec<MessageRecord>> {
    let mut stmt = conn
        .prepare(
            "SELECT m.id, m.agent_id, m.role, m.content, m.tokens_in, m.tokens_out, m.cost, m.model, m.kind, m.created_at
             FROM messages_fts f
             JOIN messages m ON m.rowid = f.rowid
             WHERE messages_fts MATCH ?1
             ORDER BY rank
             LIMIT ?2",
        )
        .map_err(|e| SessionError::Storage(harness_storage::StorageError::Sqlite(e)))?;
    let iter = stmt
        .query_map(
            params![query, i64::try_from(limit).unwrap_or(i64::MAX)],
            row_to_message,
        )
        .map_err(|e| SessionError::Storage(harness_storage::StorageError::Sqlite(e)))?;
    let mut out = Vec::new();
    for r in iter {
        out.push(r.map_err(|e| SessionError::Storage(harness_storage::StorageError::Sqlite(e)))?);
    }
    Ok(out)
}

fn row_to_message(row: &rusqlite::Row) -> rusqlite::Result<MessageRecord> {
    let id_s: String = row.get(0)?;
    let aid_s: String = row.get(1)?;
    let role_s: String = row.get(2)?;
    let to_uuid = |s: &str| {
        uuid::Uuid::parse_str(s).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
        })
    };
    let kind_s: String = row.get(8)?;
    Ok(MessageRecord {
        id: MessageId::from(to_uuid(&id_s)?),
        agent_id: AgentId::from(to_uuid(&aid_s)?),
        role: MessageRole::parse(&role_s).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(
                2,
                rusqlite::types::Type::Text,
                Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    e.to_string(),
                )),
            )
        })?,
        content: row.get(3)?,
        tokens_in: row.get(4)?,
        tokens_out: row.get(5)?,
        cost: row.get(6)?,
        model: row.get(7)?,
        kind: MessageKind::parse_or_chat(&kind_s),
        created_at: row.get(9)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{self, NewAgent};
    use crate::manager::SessionManager;
    use harness_storage::{Database, Writer};
    use tempfile::NamedTempFile;

    async fn setup() -> (NamedTempFile, WriterHandle, AgentId) {
        let f = NamedTempFile::new().unwrap();
        Database::open(f.path()).unwrap();
        let h = Writer::spawn(f.path()).unwrap();
        let mgr = SessionManager::new(&h);
        let s = mgr.create("/tmp", None).await.unwrap();
        let aid = agent::insert(
            &h,
            NewAgent {
                session_id: s.id,
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
        (f, h, aid)
    }

    #[tokio::test]
    async fn insert_and_list() {
        let (f, w, aid) = setup().await;
        insert(
            &w,
            NewMessage {
                agent_id: aid,
                role: MessageRole::User,
                content: Some("hello".into()),
                model: None,
                kind: MessageKind::Chat,
            },
        )
        .await
        .unwrap();
        insert(
            &w,
            NewMessage {
                agent_id: aid,
                role: MessageRole::Assistant,
                content: Some("world".into()),
                model: Some("claude-sonnet-4-6".into()),
                kind: MessageKind::Chat,
            },
        )
        .await
        .unwrap();

        let reader = rusqlite::Connection::open(f.path()).unwrap();
        let msgs = list_for_agent(&reader, aid).unwrap();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].role, MessageRole::User);
        assert_eq!(msgs[1].content.as_deref(), Some("world"));
    }

    #[tokio::test]
    async fn append_accumulates() {
        let (f, w, aid) = setup().await;
        let (id, _) = insert(
            &w,
            NewMessage {
                agent_id: aid,
                role: MessageRole::Assistant,
                content: Some(String::new()),
                model: Some("m".into()),
                kind: MessageKind::Chat,
            },
        )
        .await
        .unwrap();
        append_content(&w, id, "hel".into(), None, None, None)
            .await
            .unwrap();
        append_content(&w, id, "lo".into(), Some(10), Some(20), Some(0.001))
            .await
            .unwrap();

        let reader = rusqlite::Connection::open(f.path()).unwrap();
        let msgs = list_for_agent(&reader, aid).unwrap();
        assert_eq!(msgs[0].content.as_deref(), Some("hello"));
        assert_eq!(msgs[0].tokens_in, Some(10));
        assert_eq!(msgs[0].tokens_out, Some(20));
    }

    #[tokio::test]
    async fn fts_search_finds_inserted_content() {
        let (f, w, aid) = setup().await;
        insert(
            &w,
            NewMessage {
                agent_id: aid,
                role: MessageRole::User,
                content: Some("please refactor the authentication module".into()),
                model: None,
                kind: MessageKind::Chat,
            },
        )
        .await
        .unwrap();

        let reader = rusqlite::Connection::open(f.path()).unwrap();
        let hits = search(&reader, "authentication", 10).unwrap();
        assert_eq!(hits.len(), 1);
    }
}
