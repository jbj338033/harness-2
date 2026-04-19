use crate::{Result, WriterHandle};
use rusqlite::{Connection, params};

pub fn get(conn: &Connection, key: &str) -> Result<Option<String>> {
    let mut stmt = conn.prepare("SELECT value FROM config WHERE key = ?1")?;
    let mut rows = stmt.query(params![key])?;
    match rows.next()? {
        Some(row) => Ok(Some(row.get::<_, String>(0)?)),
        None => Ok(None),
    }
}

pub fn list(conn: &Connection) -> Result<Vec<(String, String)>> {
    let mut stmt = conn.prepare("SELECT key, value FROM config ORDER BY key")?;
    let iter = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    let mut out = Vec::new();
    for row in iter {
        out.push(row?);
    }
    Ok(out)
}

pub async fn set(writer: &WriterHandle, key: String, value: String) -> Result<()> {
    writer
        .execute(move |c| {
            c.execute(
                "INSERT INTO config (key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![key, value],
            )?;
            Ok(())
        })
        .await
}

pub async fn unset(writer: &WriterHandle, key: String) -> Result<bool> {
    let n = writer
        .execute(move |c| Ok(c.execute("DELETE FROM config WHERE key = ?1", params![key])?))
        .await?;
    Ok(n > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Database, Writer};
    use tempfile::NamedTempFile;

    fn setup() -> (NamedTempFile, WriterHandle) {
        let f = NamedTempFile::new().unwrap();
        Database::open(f.path()).unwrap();
        let w = Writer::spawn(f.path()).unwrap();
        (f, w)
    }

    #[tokio::test]
    async fn set_then_get_roundtrips() {
        let (f, w) = setup();
        set(&w, "network.ws_port".into(), "9000".into())
            .await
            .unwrap();
        let c = Connection::open(f.path()).unwrap();
        assert_eq!(get(&c, "network.ws_port").unwrap().as_deref(), Some("9000"));
    }

    #[tokio::test]
    async fn set_overwrites_existing_value() {
        let (f, w) = setup();
        set(&w, "k".into(), "1".into()).await.unwrap();
        set(&w, "k".into(), "2".into()).await.unwrap();
        let c = Connection::open(f.path()).unwrap();
        assert_eq!(get(&c, "k").unwrap().as_deref(), Some("2"));
    }

    #[tokio::test]
    async fn unset_removes_key() {
        let (f, w) = setup();
        set(&w, "k".into(), "v".into()).await.unwrap();
        assert!(unset(&w, "k".into()).await.unwrap());
        let c = Connection::open(f.path()).unwrap();
        assert!(get(&c, "k").unwrap().is_none());
    }

    #[tokio::test]
    async fn list_returns_sorted() {
        let (f, w) = setup();
        set(&w, "b".into(), "2".into()).await.unwrap();
        set(&w, "a".into(), "1".into()).await.unwrap();
        let c = Connection::open(f.path()).unwrap();
        let all = list(&c).unwrap();
        assert_eq!(
            all,
            vec![("a".into(), "1".into()), ("b".into(), "2".into())]
        );
    }
}
