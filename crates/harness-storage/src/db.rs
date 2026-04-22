use crate::{Result, migrations};
use rusqlite::Connection;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct Database {
    pub path: String,
    reader: Arc<Mutex<Connection>>,
}

impl Database {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let mut conn = Connection::open(path)?;
        configure(&mut conn)?;
        migrations::apply(&mut conn)?;

        let reader = Connection::open(path)?;
        configure_readonly(&reader)?;

        Ok(Self {
            path: path.to_string_lossy().into_owned(),
            reader: Arc::new(Mutex::new(reader)),
        })
    }

    #[must_use]
    pub fn reader(&self) -> Arc<Mutex<Connection>> {
        self.reader.clone()
    }
}

pub fn open_in_memory() -> Result<Database> {
    let mut conn = Connection::open_in_memory()?;
    configure(&mut conn)?;
    migrations::apply(&mut conn)?;
    Ok(Database {
        path: ":memory:".into(),
        reader: Arc::new(Mutex::new(conn)),
    })
}

pub fn open(path: impl AsRef<Path>) -> Result<Database> {
    Database::open(path)
}

// IMPLEMENTS: D-450
pub fn configure(conn: &mut Connection) -> Result<()> {
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "busy_timeout", 5000i64)?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    // Streaming LLM output flushes ~every 500ms; per-commit fsync of FULL
    // would dominate latency so D-450 keeps NORMAL and supersedes D-064.
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    // Auto-checkpoint after ~1000 frames (~4MB) keeps the WAL bounded.
    conn.pragma_update(None, "wal_autocheckpoint", 1000i64)?;
    // Refuse to grow the journal past 64MB regardless of mid-flight writes.
    conn.pragma_update(None, "journal_size_limit", 64 * 1024 * 1024i64)?;
    // 256MB mmap window shaves ~2x off random-read latency on large dbs.
    conn.pragma_update(None, "mmap_size", 256 * 1024 * 1024i64)?;
    // 64MB page cache (negative = KB) — covers hot session in RAM.
    conn.pragma_update(None, "cache_size", -65_536i64)?;
    conn.pragma_update(None, "temp_store", "MEMORY")?;
    // Let SQLite re-plan stale query plans the first time we open.
    conn.execute_batch("PRAGMA optimize")?;
    Ok(())
}

pub(crate) fn configure_readonly(conn: &Connection) -> Result<()> {
    conn.pragma_update(None, "busy_timeout", 5000i64)?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn open_creates_fresh_db() {
        let f = NamedTempFile::new().unwrap();
        let db = Database::open(f.path()).unwrap();
        let reader = db.reader.lock().await;
        let version: i64 = reader
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert!(version > 0);
    }

    #[test]
    fn wal_mode_is_set() {
        let f = NamedTempFile::new().unwrap();
        let db = Database::open(f.path()).unwrap();
        let reader = db.reader.try_lock().unwrap();
        let mode: String = reader
            .pragma_query_value(None, "journal_mode", |row| row.get(0))
            .unwrap();
        assert_eq!(mode.to_lowercase(), "wal");
    }

    #[test]
    fn in_memory_open() {
        let db = open_in_memory().unwrap();
        assert_eq!(db.path, ":memory:");
    }

    #[test]
    fn pragmas_set_per_d_450() {
        let f = NamedTempFile::new().unwrap();
        let mut conn = Connection::open(f.path()).unwrap();
        configure(&mut conn).unwrap();

        let synchronous: i64 = conn
            .pragma_query_value(None, "synchronous", |row| row.get(0))
            .unwrap();
        assert_eq!(synchronous, 1, "synchronous must stay NORMAL (=1)");

        let autocp: i64 = conn
            .pragma_query_value(None, "wal_autocheckpoint", |row| row.get(0))
            .unwrap();
        assert_eq!(autocp, 1000);

        let journal_limit: i64 = conn
            .pragma_query_value(None, "journal_size_limit", |row| row.get(0))
            .unwrap();
        assert_eq!(journal_limit, 64 * 1024 * 1024);

        let mmap_size: i64 = conn
            .pragma_query_value(None, "mmap_size", |row| row.get(0))
            .unwrap();
        assert_eq!(mmap_size, 256 * 1024 * 1024);

        let cache_size: i64 = conn
            .pragma_query_value(None, "cache_size", |row| row.get(0))
            .unwrap();
        assert_eq!(cache_size, -65_536);

        let temp_store: i64 = conn
            .pragma_query_value(None, "temp_store", |row| row.get(0))
            .unwrap();
        assert_eq!(temp_store, 2, "temp_store=MEMORY → 2");
    }
}
