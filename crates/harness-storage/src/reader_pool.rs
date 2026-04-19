use crate::{Reader, Result};
use std::path::PathBuf;
use std::sync::Mutex;

pub const DEFAULT_MAX_IDLE: usize = 8;

pub struct ReaderPool {
    db_path: PathBuf,
    idle: Mutex<Vec<Reader>>,
    max_idle: usize,
}

impl ReaderPool {
    #[must_use]
    pub fn new(db_path: PathBuf, max_idle: usize) -> Self {
        Self {
            db_path,
            idle: Mutex::new(Vec::with_capacity(max_idle)),
            max_idle,
        }
    }

    #[must_use]
    pub fn with_defaults(db_path: PathBuf) -> Self {
        Self::new(db_path, DEFAULT_MAX_IDLE)
    }

    pub fn get(&self) -> Result<PooledReader<'_>> {
        let reader = match self.idle.lock().expect("reader pool poisoned").pop() {
            Some(r) => r,
            None => Reader::open(&self.db_path)?,
        };
        Ok(PooledReader {
            reader: Some(reader),
            pool: self,
        })
    }

    #[must_use]
    pub fn idle_len(&self) -> usize {
        self.idle.lock().expect("reader pool poisoned").len()
    }
}

pub struct PooledReader<'a> {
    reader: Option<Reader>,
    pool: &'a ReaderPool,
}

impl PooledReader<'_> {
    #[must_use]
    pub fn conn(&self) -> &rusqlite::Connection {
        self.reader
            .as_ref()
            .expect("reader present until drop")
            .conn()
    }
}

impl std::ops::Deref for PooledReader<'_> {
    type Target = rusqlite::Connection;
    fn deref(&self) -> &Self::Target {
        self.conn()
    }
}

impl Drop for PooledReader<'_> {
    fn drop(&mut self) {
        if let Some(reader) = self.reader.take() {
            let mut idle = self.pool.idle.lock().expect("reader pool poisoned");
            if idle.len() < self.pool.max_idle {
                idle.push(reader);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{db, migrations};
    use rusqlite::Connection;
    use tempfile::NamedTempFile;

    fn fresh_db() -> NamedTempFile {
        let f = NamedTempFile::new().unwrap();
        let mut conn = Connection::open(f.path()).unwrap();
        db::configure(&mut conn).unwrap();
        migrations::apply(&mut conn).unwrap();
        f
    }

    #[test]
    fn checkout_returns_to_pool() {
        let f = fresh_db();
        let pool = ReaderPool::new(f.path().to_path_buf(), 4);
        {
            let r = pool.get().unwrap();
            let _: i64 = r
                .conn()
                .pragma_query_value(None, "user_version", |row| row.get(0))
                .unwrap();
        }
        assert_eq!(pool.idle_len(), 1);
    }

    #[test]
    fn idle_capacity_is_respected() {
        let f = fresh_db();
        let pool = ReaderPool::new(f.path().to_path_buf(), 2);
        let r1 = pool.get().unwrap();
        let r2 = pool.get().unwrap();
        let r3 = pool.get().unwrap();
        drop(r1);
        drop(r2);
        drop(r3);
        assert_eq!(pool.idle_len(), 2);
    }

    #[test]
    fn concurrent_checkouts_open_fresh_connections() {
        let f = fresh_db();
        let pool = ReaderPool::new(f.path().to_path_buf(), 4);
        let r1 = pool.get().unwrap();
        let r2 = pool.get().unwrap();
        let v1: i64 = r1
            .conn()
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        let v2: i64 = r2
            .conn()
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(v1, v2);
    }
}
