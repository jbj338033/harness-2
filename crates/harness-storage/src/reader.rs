use crate::{Result, db};
use rusqlite::Connection;
use std::path::PathBuf;

pub struct Reader {
    conn: Connection,
}

impl Reader {
    pub fn open(path: impl Into<PathBuf>) -> Result<Self> {
        let path: PathBuf = path.into();
        let conn = Connection::open(&path)?;
        db::configure_readonly(&conn)?;
        Ok(Self { conn })
    }

    #[must_use]
    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    pub fn conn_mut(&mut self) -> &mut Connection {
        &mut self.conn
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::migrations;
    use tempfile::NamedTempFile;

    #[test]
    fn open_reads_existing_schema() {
        let f = NamedTempFile::new().unwrap();
        {
            let mut conn = Connection::open(f.path()).unwrap();
            db::configure(&mut conn).unwrap();
            migrations::apply(&mut conn).unwrap();
        }
        let reader = Reader::open(f.path()).unwrap();
        let version: i64 = reader
            .conn()
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert!(version > 0);
    }
}
