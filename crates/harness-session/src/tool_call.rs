use crate::{Result, SessionError};
use harness_core::{MessageId, ToolCallId, now};
use harness_storage::WriterHandle;
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolCallRecord {
    pub id: ToolCallId,
    pub message_id: MessageId,
    pub name: String,
    pub input: Value,
    pub output: Option<String>,
    pub exit_code: Option<i64>,
    pub duration_ms: Option<i64>,
    pub created_at: i64,
}

pub async fn insert_pending(
    writer: &WriterHandle,
    message_id: MessageId,
    name: String,
    input: Value,
) -> Result<ToolCallId> {
    let id = ToolCallId::new();
    let id_s = id.as_uuid().to_string();
    let mid_s = message_id.as_uuid().to_string();
    let input_json = serde_json::to_string(&input).map_err(harness_storage::StorageError::from)?;
    let ts = now().as_millis();
    writer
        .execute(move |c| {
            c.execute(
                "INSERT INTO tool_calls
                     (id, message_id, name, input, output, exit_code, duration_ms, created_at)
                 VALUES (?1, ?2, ?3, ?4, NULL, NULL, NULL, ?5)",
                params![id_s, mid_s, name, input_json, ts],
            )?;
            Ok(())
        })
        .await?;
    Ok(id)
}

pub async fn mark_pending_as_crashed_for_agents(
    writer: &WriterHandle,
    agent_ids: Vec<harness_core::AgentId>,
) -> Result<usize> {
    if agent_ids.is_empty() {
        return Ok(0);
    }
    let total = writer
        .execute(move |c| {
            let mut total: usize = 0;
            for aid in &agent_ids {
                let aid_s = aid.as_uuid().to_string();
                let n = c.execute(
                    "UPDATE tool_calls
                        SET output = COALESCE(output, 'crashed before completion'),
                            exit_code = COALESCE(exit_code, -1)
                      WHERE output IS NULL
                        AND message_id IN (SELECT id FROM messages WHERE agent_id = ?1)",
                    params![aid_s],
                )?;
                total += n;
            }
            Ok(total)
        })
        .await?;
    Ok(total)
}

pub async fn record_result(
    writer: &WriterHandle,
    id: ToolCallId,
    output: String,
    exit_code: Option<i64>,
    duration_ms: Option<i64>,
) -> Result<()> {
    let id_s = id.as_uuid().to_string();
    writer
        .execute(move |c| {
            let affected = c.execute(
                "UPDATE tool_calls
                    SET output      = ?1,
                        exit_code   = ?2,
                        duration_ms = ?3
                  WHERE id = ?4",
                params![output, exit_code, duration_ms, id_s],
            )?;
            if affected == 0 {
                return Err(harness_storage::StorageError::NotFound(format!(
                    "tool_call {id_s}"
                )));
            }
            Ok(())
        })
        .await?;
    Ok(())
}

pub fn list_for_message(conn: &Connection, message: MessageId) -> Result<Vec<ToolCallRecord>> {
    let mid = message.as_uuid().to_string();
    let mut stmt = conn
        .prepare(
            "SELECT id, message_id, name, input, output, exit_code, duration_ms, created_at
               FROM tool_calls
              WHERE message_id = ?1
              ORDER BY created_at ASC, rowid ASC",
        )
        .map_err(|e| SessionError::Storage(harness_storage::StorageError::Sqlite(e)))?;
    let iter = stmt
        .query_map(params![mid], row_to_record)
        .map_err(|e| SessionError::Storage(harness_storage::StorageError::Sqlite(e)))?;
    let mut out = Vec::new();
    for r in iter {
        out.push(r.map_err(|e| SessionError::Storage(harness_storage::StorageError::Sqlite(e)))?);
    }
    Ok(out)
}

fn row_to_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<ToolCallRecord> {
    let id_s: String = row.get(0)?;
    let mid_s: String = row.get(1)?;
    let input_s: String = row.get(3)?;
    let to_uuid = |s: &str| {
        uuid::Uuid::parse_str(s).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
        })
    };
    let input: Value = serde_json::from_str(&input_s).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(3, rusqlite::types::Type::Text, Box::new(e))
    })?;
    Ok(ToolCallRecord {
        id: ToolCallId::from(to_uuid(&id_s)?),
        message_id: MessageId::from(to_uuid(&mid_s)?),
        name: row.get(2)?,
        input,
        output: row.get(4)?,
        exit_code: row.get(5)?,
        duration_ms: row.get(6)?,
        created_at: row.get(7)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{self, NewAgent};
    use crate::manager::SessionManager;
    use crate::message::{self, MessageRole, NewMessage};
    use harness_storage::{Database, Writer};
    use serde_json::json;
    use tempfile::NamedTempFile;

    async fn setup_message() -> (NamedTempFile, WriterHandle, MessageId) {
        let f = NamedTempFile::new().unwrap();
        Database::open(f.path()).unwrap();
        let w = Writer::spawn(f.path()).unwrap();
        let sm = SessionManager::new(&w);
        let s = sm.create("/tmp", None).await.unwrap();
        let aid = agent::insert(
            &w,
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
        let (mid, _) = message::insert(
            &w,
            NewMessage {
                agent_id: aid,
                role: MessageRole::Assistant,
                content: Some(String::new()),
                model: Some("m".into()),
                kind: crate::message::MessageKind::Chat,
            },
        )
        .await
        .unwrap();
        (f, w, mid)
    }

    #[tokio::test]
    async fn insert_then_record_roundtrip() {
        let (f, w, mid) = setup_message().await;
        let id = insert_pending(&w, mid, "bash".into(), json!({"cmd": "ls"}))
            .await
            .unwrap();
        record_result(&w, id, "out".into(), Some(0), Some(42))
            .await
            .unwrap();

        let reader = rusqlite::Connection::open(f.path()).unwrap();
        let rows = list_for_message(&reader, mid).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].name, "bash");
        assert_eq!(rows[0].input["cmd"], "ls");
        assert_eq!(rows[0].output.as_deref(), Some("out"));
        assert_eq!(rows[0].exit_code, Some(0));
        assert_eq!(rows[0].duration_ms, Some(42));
    }

    #[tokio::test]
    async fn record_result_errors_on_missing_id() {
        let (_f, w, _mid) = setup_message().await;
        let fake = ToolCallId::new();
        let err = record_result(&w, fake, "x".into(), None, None)
            .await
            .unwrap_err();
        assert!(matches!(err, SessionError::Storage(_)));
    }

    #[tokio::test]
    async fn list_orders_by_created_at() {
        let (f, w, mid) = setup_message().await;
        let a = insert_pending(&w, mid, "a".into(), json!({}))
            .await
            .unwrap();
        let b = insert_pending(&w, mid, "b".into(), json!({}))
            .await
            .unwrap();
        record_result(&w, a, "out-a".into(), None, None)
            .await
            .unwrap();
        record_result(&w, b, "out-b".into(), None, None)
            .await
            .unwrap();

        let reader = rusqlite::Connection::open(f.path()).unwrap();
        let rows = list_for_message(&reader, mid).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].name, "a");
        assert_eq!(rows[1].name, "b");
    }

    #[tokio::test]
    async fn pending_call_has_null_output() {
        let (f, w, mid) = setup_message().await;
        insert_pending(&w, mid, "p".into(), json!({}))
            .await
            .unwrap();
        let reader = rusqlite::Connection::open(f.path()).unwrap();
        let rows = list_for_message(&reader, mid).unwrap();
        assert!(rows[0].output.is_none());
        assert!(rows[0].exit_code.is_none());
    }
}
