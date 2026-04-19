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

pub(crate) fn configure(conn: &mut Connection) -> Result<()> {
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "busy_timeout", 5000i64)?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
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
}
