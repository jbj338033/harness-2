// IMPLEMENTS: D-205
use crate::{Result, WriterHandle};
use harness_core::now;
use rusqlite::{Connection, params};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct TrustedWorkspace {
    pub path: String,
    pub trusted: bool,
    pub trusted_at: i64,
}

pub fn canonicalize(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

pub fn is_trusted(conn: &Connection, path: &Path) -> Result<bool> {
    let canonical = canonicalize(path);
    let key = canonical.to_string_lossy().into_owned();
    let mut stmt = conn.prepare("SELECT trusted FROM workspaces WHERE path = ?1")?;
    let mut rows = stmt.query(params![key])?;
    match rows.next()? {
        Some(row) => Ok(row.get::<_, i64>(0)? != 0),
        None => Ok(false),
    }
}

pub fn lookup(conn: &Connection, path: &Path) -> Result<Option<TrustedWorkspace>> {
    let canonical = canonicalize(path);
    let key = canonical.to_string_lossy().into_owned();
    let mut stmt =
        conn.prepare("SELECT path, trusted, trusted_at FROM workspaces WHERE path = ?1")?;
    let mut rows = stmt.query(params![key])?;
    match rows.next()? {
        Some(row) => Ok(Some(TrustedWorkspace {
            path: row.get(0)?,
            trusted: row.get::<_, i64>(1)? != 0,
            trusted_at: row.get(2)?,
        })),
        None => Ok(None),
    }
}

pub fn list(conn: &Connection) -> Result<Vec<TrustedWorkspace>> {
    let mut stmt =
        conn.prepare("SELECT path, trusted, trusted_at FROM workspaces ORDER BY path")?;
    let iter = stmt.query_map([], |row| {
        Ok(TrustedWorkspace {
            path: row.get(0)?,
            trusted: row.get::<_, i64>(1)? != 0,
            trusted_at: row.get(2)?,
        })
    })?;
    let mut out = Vec::new();
    for row in iter {
        out.push(row?);
    }
    Ok(out)
}

pub async fn trust(writer: &WriterHandle, path: PathBuf) -> Result<()> {
    let canonical = canonicalize(&path);
    let key = canonical.to_string_lossy().into_owned();
    let ts = now().as_millis();
    writer
        .execute(move |c| {
            c.execute(
                "INSERT INTO workspaces (path, trusted, trusted_at) VALUES (?1, 1, ?2)
                 ON CONFLICT(path) DO UPDATE SET trusted = 1, trusted_at = excluded.trusted_at",
                params![key, ts],
            )?;
            Ok(())
        })
        .await
}

pub async fn untrust(writer: &WriterHandle, path: PathBuf) -> Result<bool> {
    let canonical = canonicalize(&path);
    let key = canonical.to_string_lossy().into_owned();
    let n = writer
        .execute(move |c| Ok(c.execute("DELETE FROM workspaces WHERE path = ?1", params![key])?))
        .await?;
    Ok(n > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Database, Writer};
    use tempfile::{NamedTempFile, TempDir};

    fn setup() -> (NamedTempFile, WriterHandle) {
        let f = NamedTempFile::new().unwrap();
        Database::open(f.path()).unwrap();
        let w = Writer::spawn(f.path()).unwrap();
        (f, w)
    }

    #[tokio::test]
    async fn untrusted_workspace_starts_false() {
        let (f, _w) = setup();
        let dir = TempDir::new().unwrap();
        let c = Connection::open(f.path()).unwrap();
        assert!(!is_trusted(&c, dir.path()).unwrap());
    }

    #[tokio::test]
    async fn trust_then_query_returns_true() {
        let (f, w) = setup();
        let dir = TempDir::new().unwrap();
        trust(&w, dir.path().to_path_buf()).await.unwrap();
        let c = Connection::open(f.path()).unwrap();
        assert!(is_trusted(&c, dir.path()).unwrap());
    }

    #[tokio::test]
    async fn untrust_removes_grant() {
        let (f, w) = setup();
        let dir = TempDir::new().unwrap();
        trust(&w, dir.path().to_path_buf()).await.unwrap();
        assert!(untrust(&w, dir.path().to_path_buf()).await.unwrap());
        let c = Connection::open(f.path()).unwrap();
        assert!(!is_trusted(&c, dir.path()).unwrap());
    }

    #[tokio::test]
    async fn untrust_returns_false_when_no_row_existed() {
        let (_f, w) = setup();
        let dir = TempDir::new().unwrap();
        assert!(!untrust(&w, dir.path().to_path_buf()).await.unwrap());
    }

    #[tokio::test]
    async fn trust_is_idempotent_and_updates_timestamp() {
        let (f, w) = setup();
        let dir = TempDir::new().unwrap();
        trust(&w, dir.path().to_path_buf()).await.unwrap();
        let c = Connection::open(f.path()).unwrap();
        let first = lookup(&c, dir.path()).unwrap().unwrap().trusted_at;
        std::thread::sleep(std::time::Duration::from_millis(2));
        trust(&w, dir.path().to_path_buf()).await.unwrap();
        let c = Connection::open(f.path()).unwrap();
        let second = lookup(&c, dir.path()).unwrap().unwrap().trusted_at;
        assert!(second >= first);
        assert_eq!(list(&c).unwrap().len(), 1);
    }

    #[tokio::test]
    async fn list_returns_sorted_by_path() {
        let (f, w) = setup();
        let dir_a = TempDir::new().unwrap();
        let dir_b = TempDir::new().unwrap();
        trust(&w, dir_b.path().to_path_buf()).await.unwrap();
        trust(&w, dir_a.path().to_path_buf()).await.unwrap();
        let c = Connection::open(f.path()).unwrap();
        let all = list(&c).unwrap();
        assert_eq!(all.len(), 2);
        assert!(all[0].path < all[1].path);
    }

    #[test]
    fn canonicalize_falls_back_when_path_missing() {
        let missing = std::path::Path::new("/this/path/should/not/exist/qwerty");
        let out = canonicalize(missing);
        assert_eq!(out, missing);
    }
}
