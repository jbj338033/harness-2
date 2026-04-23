// IMPLEMENTS: D-031, D-033, D-034, D-035, D-036, D-037, D-038, D-041, D-042, D-076, D-077
use crate::{Result, WriterHandle};
use harness_core::now;
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::str::FromStr;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    Perceive,
    Think,
    Act,
    Observe,
    Remember,
    Recall,
    Plan,
    Verify,
    Trigger,
    Cancel,
    Revise,
    MessageUser,
    MessageAssistant,
    MessageSystem,
    ToolCall,
    ToolResult,
}

impl EventKind {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Perceive => "perceive",
            Self::Think => "think",
            Self::Act => "act",
            Self::Observe => "observe",
            Self::Remember => "remember",
            Self::Recall => "recall",
            Self::Plan => "plan",
            Self::Verify => "verify",
            Self::Trigger => "trigger",
            Self::Cancel => "cancel",
            Self::Revise => "revise",
            Self::MessageUser => "message_user",
            Self::MessageAssistant => "message_assistant",
            Self::MessageSystem => "message_system",
            Self::ToolCall => "tool_call",
            Self::ToolResult => "tool_result",
        }
    }
}

impl FromStr for EventKind {
    type Err = String;

    fn from_str(value: &str) -> std::result::Result<Self, Self::Err> {
        let kind = match value {
            "perceive" => Self::Perceive,
            "think" => Self::Think,
            "act" => Self::Act,
            "observe" => Self::Observe,
            "remember" => Self::Remember,
            "recall" => Self::Recall,
            "plan" => Self::Plan,
            "verify" => Self::Verify,
            "trigger" => Self::Trigger,
            "cancel" => Self::Cancel,
            "revise" => Self::Revise,
            "message_user" => Self::MessageUser,
            "message_assistant" => Self::MessageAssistant,
            "message_system" => Self::MessageSystem,
            "tool_call" => Self::ToolCall,
            "tool_result" => Self::ToolResult,
            other => return Err(format!("unknown event kind: {other}")),
        };
        Ok(kind)
    }
}

#[derive(Debug, Clone)]
pub struct AppendEvent {
    pub session_id: String,
    pub actor: String,
    pub kind: EventKind,
    pub correlation_id: Option<String>,
    pub causation_id: Option<String>,
    pub payload: Value,
}

#[derive(Debug, Clone)]
pub struct StoredEvent {
    pub id: String,
    pub session_id: String,
    pub actor: String,
    pub kind: EventKind,
    pub correlation_id: Option<String>,
    pub causation_id: Option<String>,
    pub payload: Value,
    pub created_at: i64,
}

pub async fn append(writer: &WriterHandle, event: AppendEvent) -> Result<String> {
    let id = Uuid::now_v7().as_simple().to_string();
    let kind_str = event.kind.as_str().to_string();
    let payload_str = event.payload.to_string();
    let ts = now().as_millis();
    let inserted = id.clone();
    let session_id = event.session_id;
    let actor = event.actor;
    let correlation_id = event.correlation_id;
    let causation_id = event.causation_id;

    writer
        .execute(move |conn| {
            conn.execute(
                "INSERT INTO events (id, session_id, actor, kind, correlation_id, causation_id, payload, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    inserted,
                    session_id,
                    actor,
                    kind_str,
                    correlation_id,
                    causation_id,
                    payload_str,
                    ts
                ],
            )?;
            Ok(())
        })
        .await?;

    Ok(id)
}

pub fn for_session(conn: &Connection, session_id: &str) -> Result<Vec<StoredEvent>> {
    let mut stmt = conn.prepare(
        "SELECT id, session_id, actor, kind, correlation_id, causation_id, payload, created_at
         FROM events
         WHERE session_id = ?1
         ORDER BY created_at, id",
    )?;
    rows_to_events(stmt.query_map(params![session_id], row_to_event)?)
}

pub fn for_correlation(conn: &Connection, correlation_id: &str) -> Result<Vec<StoredEvent>> {
    let mut stmt = conn.prepare(
        "SELECT id, session_id, actor, kind, correlation_id, causation_id, payload, created_at
         FROM events
         WHERE correlation_id = ?1
         ORDER BY created_at, id",
    )?;
    rows_to_events(stmt.query_map(params![correlation_id], row_to_event)?)
}

pub fn lookup(conn: &Connection, id: &str) -> Result<Option<StoredEvent>> {
    let mut stmt = conn.prepare(
        "SELECT id, session_id, actor, kind, correlation_id, causation_id, payload, created_at
         FROM events WHERE id = ?1",
    )?;
    let mut rows = stmt.query(params![id])?;
    match rows.next()? {
        Some(row) => Ok(Some(row_to_event_owned(row)?)),
        None => Ok(None),
    }
}

pub fn count_session(conn: &Connection, session_id: &str) -> Result<i64> {
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM events WHERE session_id = ?1",
        params![session_id],
        |r| r.get(0),
    )?)
}

// IMPLEMENTS: D-448
pub fn count(conn: &Connection) -> Result<i64> {
    Ok(conn.query_row("SELECT COUNT(*) FROM events", [], |r| r.get(0))?)
}

#[derive(Debug, Clone, Copy, Default)]
pub struct BackfillStats {
    pub messages: usize,
    pub tool_calls: usize,
}

pub fn backfill(tx: &rusqlite::Transaction<'_>) -> Result<BackfillStats> {
    Ok(BackfillStats {
        messages: backfill_messages(tx)?,
        tool_calls: backfill_tool_calls(tx)?,
    })
}

fn backfill_messages(tx: &rusqlite::Transaction<'_>) -> Result<usize> {
    let mut stmt = tx.prepare(
        "SELECT m.id, a.session_id, m.role, m.content, m.tokens_in, m.tokens_out,
                m.cost, m.model, m.kind, m.created_at, m.agent_id
         FROM messages m
         JOIN agents a ON a.id = m.agent_id
         WHERE NOT EXISTS (SELECT 1 FROM events e WHERE e.id = m.id)",
    )?;
    let mut rows = stmt.query([])?;
    let mut inserted = 0usize;
    while let Some(row) = rows.next()? {
        let id: String = row.get(0)?;
        let session_id: String = row.get(1)?;
        let role: String = row.get(2)?;
        let content: Option<String> = row.get(3)?;
        let tokens_in: Option<i64> = row.get(4)?;
        let tokens_out: Option<i64> = row.get(5)?;
        let cost: Option<f64> = row.get(6)?;
        let model: Option<String> = row.get(7)?;
        let kind_str: String = row.get(8)?;
        let created_at: i64 = row.get(9)?;
        let agent_id: String = row.get(10)?;

        let event_kind = match role.as_str() {
            "user" => EventKind::MessageUser,
            "assistant" => EventKind::MessageAssistant,
            _ => EventKind::MessageSystem,
        };
        let actor = match role.as_str() {
            "user" => "human:user".to_string(),
            "assistant" => format!("agent:{agent_id}"),
            _ => "system:harness".to_string(),
        };
        let payload = serde_json::json!({
            "schema_version": 1,
            "content": content,
            "tokens_in": tokens_in,
            "tokens_out": tokens_out,
            "cost": cost,
            "model": model,
            "kind": kind_str,
            "agent_id": agent_id,
        });

        tx.execute(
            "INSERT INTO events (id, session_id, actor, kind, correlation_id, causation_id, payload, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, NULL, ?6, ?7)",
            params![
                id,
                session_id,
                actor,
                event_kind.as_str(),
                agent_id,
                payload.to_string(),
                created_at
            ],
        )?;
        inserted += 1;
    }
    Ok(inserted)
}

fn backfill_tool_calls(tx: &rusqlite::Transaction<'_>) -> Result<usize> {
    let mut stmt = tx.prepare(
        "SELECT t.id, a.session_id, t.message_id, t.name, t.input, t.output,
                t.exit_code, t.duration_ms, t.created_at
         FROM tool_calls t
         JOIN messages m ON m.id = t.message_id
         JOIN agents a ON a.id = m.agent_id
         WHERE NOT EXISTS (SELECT 1 FROM events e WHERE e.id = t.id)",
    )?;
    let mut rows = stmt.query([])?;
    let mut inserted = 0usize;
    while let Some(row) = rows.next()? {
        let id: String = row.get(0)?;
        let session_id: String = row.get(1)?;
        let message_id: String = row.get(2)?;
        let name: String = row.get(3)?;
        let input: String = row.get(4)?;
        let output: Option<String> = row.get(5)?;
        let exit_code: Option<i64> = row.get(6)?;
        let duration_ms: Option<i64> = row.get(7)?;
        let created_at: i64 = row.get(8)?;

        let payload = serde_json::json!({
            "schema_version": 1,
            "name": name,
            "input": serde_json::from_str::<Value>(&input).unwrap_or(Value::String(input)),
            "output": output,
            "exit_code": exit_code,
            "duration_ms": duration_ms,
        });

        tx.execute(
            "INSERT INTO events (id, session_id, actor, kind, correlation_id, causation_id, payload, created_at)
             VALUES (?1, ?2, ?3, 'tool_call', ?4, ?5, ?6, ?7)",
            params![
                id,
                session_id,
                format!("tool:{name}"),
                message_id,
                message_id,
                payload.to_string(),
                created_at
            ],
        )?;
        inserted += 1;
    }
    Ok(inserted)
}

fn row_to_event(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredEvent> {
    let kind_str: String = row.get(3)?;
    let kind = EventKind::from_str(&kind_str).map_err(|msg| {
        rusqlite::Error::FromSqlConversionFailure(
            3,
            rusqlite::types::Type::Text,
            Box::new(EventKindParseError(msg)),
        )
    })?;
    let payload_str: String = row.get(6)?;
    let payload: Value = serde_json::from_str(&payload_str).unwrap_or(Value::Null);
    Ok(StoredEvent {
        id: row.get(0)?,
        session_id: row.get(1)?,
        actor: row.get(2)?,
        kind,
        correlation_id: row.get(4)?,
        causation_id: row.get(5)?,
        payload,
        created_at: row.get(7)?,
    })
}

fn row_to_event_owned(row: &rusqlite::Row<'_>) -> Result<StoredEvent> {
    Ok(row_to_event(row)?)
}

fn rows_to_events<'a, I>(iter: I) -> Result<Vec<StoredEvent>>
where
    I: Iterator<Item = rusqlite::Result<StoredEvent>> + 'a,
{
    let mut out = Vec::new();
    for row in iter {
        out.push(row?);
    }
    Ok(out)
}

#[derive(Debug)]
struct EventKindParseError(String);

impl std::fmt::Display for EventKindParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for EventKindParseError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Database, Writer};
    use serde_json::json;
    use tempfile::NamedTempFile;

    fn setup() -> (NamedTempFile, WriterHandle) {
        let f = NamedTempFile::new().unwrap();
        Database::open(f.path()).unwrap();
        let w = Writer::spawn(f.path()).unwrap();
        (f, w)
    }

    fn evt(session: &str, kind: EventKind, payload: Value) -> AppendEvent {
        AppendEvent {
            session_id: session.into(),
            actor: "human:user".into(),
            kind,
            correlation_id: None,
            causation_id: None,
            payload,
        }
    }

    #[tokio::test]
    async fn append_returns_uuidv7_hex() {
        let (_f, w) = setup();
        let id = append(&w, evt("s1", EventKind::Perceive, json!({"a": 1})))
            .await
            .unwrap();
        assert_eq!(id.len(), 32, "uuid simple format is 32 hex chars");
    }

    #[tokio::test]
    async fn for_session_returns_only_matching_session() {
        let (f, w) = setup();
        append(&w, evt("s1", EventKind::Perceive, json!({"x": 1})))
            .await
            .unwrap();
        append(&w, evt("s2", EventKind::Think, json!({"x": 2})))
            .await
            .unwrap();
        append(&w, evt("s1", EventKind::Act, json!({"x": 3})))
            .await
            .unwrap();
        let c = Connection::open(f.path()).unwrap();
        let s1 = for_session(&c, "s1").unwrap();
        assert_eq!(s1.len(), 2);
        assert!(s1.iter().all(|e| e.session_id == "s1"));
    }

    #[tokio::test]
    async fn for_correlation_groups_branch() {
        let (f, w) = setup();
        let mut e = evt("s1", EventKind::Plan, json!({}));
        e.correlation_id = Some("branch-a".into());
        append(&w, e).await.unwrap();
        let mut e = evt("s1", EventKind::Verify, json!({}));
        e.correlation_id = Some("branch-a".into());
        append(&w, e).await.unwrap();
        let mut e = evt("s1", EventKind::Plan, json!({}));
        e.correlation_id = Some("branch-b".into());
        append(&w, e).await.unwrap();
        let c = Connection::open(f.path()).unwrap();
        assert_eq!(for_correlation(&c, "branch-a").unwrap().len(), 2);
        assert_eq!(for_correlation(&c, "branch-b").unwrap().len(), 1);
    }

    #[tokio::test]
    async fn lookup_round_trips_payload() {
        let (f, w) = setup();
        let id = append(
            &w,
            evt("s1", EventKind::ToolCall, json!({"name": "fs.read"})),
        )
        .await
        .unwrap();
        let c = Connection::open(f.path()).unwrap();
        let row = lookup(&c, &id).unwrap().unwrap();
        assert_eq!(row.kind, EventKind::ToolCall);
        assert_eq!(row.payload, json!({"name": "fs.read"}));
    }

    #[tokio::test]
    async fn lookup_misses_unknown_id() {
        let (f, _w) = setup();
        let c = Connection::open(f.path()).unwrap();
        assert!(
            lookup(&c, "00000000000000000000000000000000")
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn count_session_matches_appends() {
        let (f, w) = setup();
        for i in 0..5 {
            append(&w, evt("s1", EventKind::Think, json!({"i": i})))
                .await
                .unwrap();
        }
        let c = Connection::open(f.path()).unwrap();
        assert_eq!(count_session(&c, "s1").unwrap(), 5);
    }

    #[tokio::test]
    async fn for_session_orders_by_created_at_then_id() {
        let (f, w) = setup();
        let id_a = append(&w, evt("s1", EventKind::Perceive, json!({"k": "a"})))
            .await
            .unwrap();
        let id_b = append(&w, evt("s1", EventKind::Perceive, json!({"k": "b"})))
            .await
            .unwrap();
        let c = Connection::open(f.path()).unwrap();
        let rows = for_session(&c, "s1").unwrap();
        assert_eq!(rows[0].id, id_a);
        assert_eq!(rows[1].id, id_b);
    }

    #[test]
    fn event_kind_round_trips_via_string() {
        for k in [
            EventKind::Perceive,
            EventKind::Think,
            EventKind::Act,
            EventKind::Observe,
            EventKind::Remember,
            EventKind::Recall,
            EventKind::Plan,
            EventKind::Verify,
            EventKind::Trigger,
            EventKind::Cancel,
            EventKind::Revise,
            EventKind::MessageUser,
            EventKind::MessageAssistant,
            EventKind::MessageSystem,
            EventKind::ToolCall,
            EventKind::ToolResult,
        ] {
            let s = k.as_str();
            assert_eq!(EventKind::from_str(s).unwrap(), k);
        }
    }

    #[test]
    fn event_kind_rejects_unknown_string() {
        assert!(EventKind::from_str("nope").is_err());
    }

    #[tokio::test]
    async fn backfill_messages_copies_rows_idempotently() {
        let (_f, w) = setup();
        let now_ts = now().as_millis();
        w.with_tx(move |tx| {
            tx.execute(
                "INSERT INTO sessions (id, cwd, created_at, updated_at) VALUES ('s1', '/tmp', ?1, ?1)",
                params![now_ts],
            )?;
            tx.execute(
                "INSERT INTO agents (id, session_id, role, model, status, created_at)
                 VALUES ('a1', 's1', 'root', 'm', 'running', ?1)",
                params![now_ts],
            )?;
            tx.execute(
                "INSERT INTO messages (id, agent_id, role, content, created_at)
                 VALUES ('m1', 'a1', 'user', 'hello', ?1)",
                params![now_ts],
            )?;
            tx.execute(
                "INSERT INTO messages (id, agent_id, role, content, created_at)
                 VALUES ('m2', 'a1', 'assistant', 'hi', ?1)",
                params![now_ts + 1],
            )?;
            Ok(())
        })
        .await
        .unwrap();

        let stats = w
            .with_tx(|tx| {
                let s = backfill(tx)?;
                Ok(s)
            })
            .await
            .unwrap();
        assert_eq!(stats.messages, 2);

        // Second run inserts nothing more
        let stats = w
            .with_tx(|tx| {
                let s = backfill(tx)?;
                Ok(s)
            })
            .await
            .unwrap();
        assert_eq!(stats.messages, 0);
    }

    #[tokio::test]
    async fn backfill_assigns_event_kind_per_role() {
        let (f, w) = setup();
        let now_ts = now().as_millis();
        w.with_tx(move |tx| {
            tx.execute(
                "INSERT INTO sessions (id, cwd, created_at, updated_at) VALUES ('s1', '/tmp', ?1, ?1)",
                params![now_ts],
            )?;
            tx.execute(
                "INSERT INTO agents (id, session_id, role, model, status, created_at)
                 VALUES ('a1', 's1', 'root', 'm', 'running', ?1)",
                params![now_ts],
            )?;
            tx.execute(
                "INSERT INTO messages (id, agent_id, role, content, created_at)
                 VALUES ('mu', 'a1', 'user', 'u', ?1)",
                params![now_ts],
            )?;
            tx.execute(
                "INSERT INTO messages (id, agent_id, role, content, created_at)
                 VALUES ('ma', 'a1', 'assistant', 'a', ?1)",
                params![now_ts + 1],
            )?;
            tx.execute(
                "INSERT INTO messages (id, agent_id, role, content, created_at)
                 VALUES ('ms', 'a1', 'system', 's', ?1)",
                params![now_ts + 2],
            )?;
            Ok(())
        })
        .await
        .unwrap();

        w.with_tx(|tx| {
            backfill(tx)?;
            Ok(())
        })
        .await
        .unwrap();

        let c = Connection::open(f.path()).unwrap();
        let row = lookup(&c, "mu").unwrap().unwrap();
        assert_eq!(row.kind, EventKind::MessageUser);
        let row = lookup(&c, "ma").unwrap().unwrap();
        assert_eq!(row.kind, EventKind::MessageAssistant);
        assert_eq!(row.actor, "agent:a1");
        let row = lookup(&c, "ms").unwrap().unwrap();
        assert_eq!(row.kind, EventKind::MessageSystem);
    }

    #[tokio::test]
    async fn backfill_tool_calls_links_to_message() {
        let (f, w) = setup();
        let now_ts = now().as_millis();
        w.with_tx(move |tx| {
            tx.execute(
                "INSERT INTO sessions (id, cwd, created_at, updated_at) VALUES ('s1', '/tmp', ?1, ?1)",
                params![now_ts],
            )?;
            tx.execute(
                "INSERT INTO agents (id, session_id, role, model, status, created_at)
                 VALUES ('a1', 's1', 'root', 'm', 'running', ?1)",
                params![now_ts],
            )?;
            tx.execute(
                "INSERT INTO messages (id, agent_id, role, content, created_at)
                 VALUES ('m1', 'a1', 'assistant', 'using tool', ?1)",
                params![now_ts],
            )?;
            tx.execute(
                "INSERT INTO tool_calls (id, message_id, name, input, output, exit_code, duration_ms, created_at)
                 VALUES ('t1', 'm1', 'fs.read', '{\"path\":\"x\"}', 'ok', 0, 12, ?1)",
                params![now_ts],
            )?;
            Ok(())
        })
        .await
        .unwrap();

        let stats = w
            .with_tx(|tx| {
                let s = backfill(tx)?;
                Ok(s)
            })
            .await
            .unwrap();
        assert_eq!(stats.tool_calls, 1);

        let c = Connection::open(f.path()).unwrap();
        let row = lookup(&c, "t1").unwrap().unwrap();
        assert_eq!(row.kind, EventKind::ToolCall);
        assert_eq!(row.actor, "tool:fs.read");
        assert_eq!(row.causation_id.as_deref(), Some("m1"));
        assert_eq!(row.correlation_id.as_deref(), Some("m1"));
    }

    #[tokio::test]
    async fn count_returns_total_event_rows() {
        let (f, w) = setup();
        for i in 0..3 {
            append(&w, evt("s1", EventKind::Perceive, json!({"i": i})))
                .await
                .unwrap();
        }
        let c = Connection::open(f.path()).unwrap();
        assert_eq!(count(&c).unwrap(), 3);
    }
}
