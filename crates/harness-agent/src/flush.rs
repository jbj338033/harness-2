use harness_core::MessageId;
use harness_session::message::append_content;
use harness_storage::WriterHandle;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::time::Instant;

pub const FLUSH_INTERVAL: Duration = Duration::from_millis(500);

pub struct Flusher {
    writer: WriterHandle,
    message_id: MessageId,
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    buffer: String,
    tokens_in: Option<i64>,
    tokens_out: Option<i64>,
    cost: Option<f64>,
    last_flush: Option<Instant>,
    finished: bool,
}

impl Flusher {
    #[must_use]
    pub fn new(writer: WriterHandle, message_id: MessageId) -> Self {
        Self {
            writer,
            message_id,
            state: Mutex::new(State::default()),
        }
    }

    pub async fn push(&self, chunk: &str) {
        let mut s = self.state.lock().await;
        s.buffer.push_str(chunk);
    }

    pub async fn set_usage(
        &self,
        tokens_in: Option<i64>,
        tokens_out: Option<i64>,
        cost: Option<f64>,
    ) {
        let mut s = self.state.lock().await;
        if tokens_in.is_some() {
            s.tokens_in = tokens_in;
        }
        if tokens_out.is_some() {
            s.tokens_out = tokens_out;
        }
        if cost.is_some() {
            s.cost = cost;
        }
    }

    pub async fn flush(&self) -> harness_session::Result<()> {
        let (buf, ti, to, cost) = {
            let mut s = self.state.lock().await;
            if s.buffer.is_empty() && s.tokens_in.is_none() && s.tokens_out.is_none() {
                return Ok(());
            }
            let buf = std::mem::take(&mut s.buffer);
            let ti = s.tokens_in.take();
            let to = s.tokens_out.take();
            let cost = s.cost.take();
            s.last_flush = Some(Instant::now());
            (buf, ti, to, cost)
        };

        append_content(&self.writer, self.message_id, buf, ti, to, cost).await?;
        Ok(())
    }

    pub async fn should_flush(&self, now: Instant) -> bool {
        let s = self.state.lock().await;
        if s.finished {
            return false;
        }
        if s.buffer.is_empty() {
            return false;
        }
        if s.buffer.len() > 16 * 1024 {
            return true;
        }
        match s.last_flush {
            None => true,
            Some(prev) => now.saturating_duration_since(prev) >= FLUSH_INTERVAL,
        }
    }

    pub async fn finish(&self) -> harness_session::Result<()> {
        self.flush().await?;
        let mut s = self.state.lock().await;
        s.finished = true;
        Ok(())
    }

    #[must_use]
    pub async fn buffered(&self) -> usize {
        self.state.lock().await.buffer.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use harness_session::{
        agent::{self, NewAgent},
        manager::SessionManager,
        message::{self, MessageRole, NewMessage},
    };
    use harness_storage::{Database, Writer};
    use tempfile::NamedTempFile;

    async fn setup_message() -> (NamedTempFile, WriterHandle, MessageId) {
        let f = NamedTempFile::new().unwrap();
        Database::open(f.path()).unwrap();
        let writer = Writer::spawn(f.path()).unwrap();
        let sm = SessionManager::new(&writer);
        let s = sm.create("/tmp", None).await.unwrap();
        let aid = agent::insert(
            &writer,
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
            &writer,
            NewMessage {
                agent_id: aid,
                role: MessageRole::Assistant,
                content: Some(String::new()),
                model: Some("m".into()),
                kind: harness_session::message::MessageKind::Chat,
            },
        )
        .await
        .unwrap();
        (f, writer, mid)
    }

    #[tokio::test]
    async fn buffer_accumulates_until_flush() {
        let (f, w, mid) = setup_message().await;
        let flusher = Flusher::new(w.clone(), mid);
        flusher.push("alpha ").await;
        flusher.push("beta").await;
        assert_eq!(flusher.buffered().await, "alpha beta".len());

        let reader = rusqlite::Connection::open(f.path()).unwrap();
        let content: String = reader
            .query_row(
                "SELECT content FROM messages WHERE id = ?1",
                rusqlite::params![mid.as_uuid().to_string()],
                |r: &rusqlite::Row<'_>| r.get::<_, String>(0),
            )
            .unwrap();
        assert_eq!(content, "");

        flusher.flush().await.unwrap();

        let reader = rusqlite::Connection::open(f.path()).unwrap();
        let content: String = reader
            .query_row(
                "SELECT content FROM messages WHERE id = ?1",
                rusqlite::params![mid.as_uuid().to_string()],
                |r: &rusqlite::Row<'_>| r.get::<_, String>(0),
            )
            .unwrap();
        assert_eq!(content, "alpha beta");
        assert_eq!(flusher.buffered().await, 0);
    }

    #[tokio::test]
    async fn should_flush_after_interval() {
        let (_f, w, mid) = setup_message().await;
        let flusher = Flusher::new(w, mid);
        flusher.push("x").await;

        let now = Instant::now();
        flusher.flush().await.unwrap();
        flusher.push("y").await;

        assert!(!flusher.should_flush(now).await);

        let later = now + FLUSH_INTERVAL + Duration::from_millis(10);
        assert!(flusher.should_flush(later).await);
    }

    type MessageRow = (String, Option<i64>, Option<i64>, Option<f64>);

    #[tokio::test]
    async fn finish_writes_remaining() {
        let (f, w, mid) = setup_message().await;
        let flusher = Flusher::new(w.clone(), mid);
        flusher.push("done").await;
        flusher.set_usage(Some(10), Some(5), Some(0.01)).await;
        flusher.finish().await.unwrap();

        let reader = rusqlite::Connection::open(f.path()).unwrap();
        let (content, ti, to, cost): MessageRow = reader
            .query_row(
                "SELECT content, tokens_in, tokens_out, cost FROM messages WHERE id = ?1",
                rusqlite::params![mid.as_uuid().to_string()],
                |r: &rusqlite::Row<'_>| -> rusqlite::Result<MessageRow> {
                    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
                },
            )
            .unwrap();
        assert_eq!(content, "done");
        assert_eq!(ti, Some(10));
        assert_eq!(to, Some(5));
        assert!((cost.unwrap() - 0.01).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn empty_flush_is_noop() {
        let (_f, w, mid) = setup_message().await;
        let flusher = Flusher::new(w, mid);
        flusher.flush().await.unwrap();
    }
}
