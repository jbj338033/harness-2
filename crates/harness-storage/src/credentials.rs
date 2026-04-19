use crate::{Result, WriterHandle};
use harness_core::now;
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Credential {
    pub id: String,
    pub provider: String,
    pub kind: String,
    pub value: String,
    pub label: Option<String>,
    pub created_at: i64,
}

pub async fn insert(
    writer: &WriterHandle,
    provider: String,
    kind: String,
    value: String,
    label: Option<String>,
) -> Result<String> {
    let id = uuid::Uuid::new_v4().to_string();
    let ts = now().as_millis();
    let id_clone = id.clone();
    writer
        .execute(move |c| {
            c.execute(
                "INSERT INTO credentials (id, provider, kind, value, label, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![id_clone, provider, kind, value, label, ts],
            )?;
            Ok(())
        })
        .await?;
    Ok(id)
}

pub async fn delete(writer: &WriterHandle, id: String) -> Result<bool> {
    let n = writer
        .execute(move |c| Ok(c.execute("DELETE FROM credentials WHERE id = ?1", params![id])?))
        .await?;
    Ok(n > 0)
}

pub fn list(conn: &Connection) -> Result<Vec<Credential>> {
    let mut stmt = conn.prepare(
        "SELECT id, provider, kind, value, label, created_at
         FROM credentials
         ORDER BY created_at DESC",
    )?;
    let iter = stmt.query_map([], |r| {
        Ok(Credential {
            id: r.get(0)?,
            provider: r.get(1)?,
            kind: r.get(2)?,
            value: r.get(3)?,
            label: r.get(4)?,
            created_at: r.get(5)?,
        })
    })?;
    let mut out = Vec::new();
    for row in iter {
        out.push(row?);
    }
    Ok(out)
}

pub async fn replace_value(writer: &WriterHandle, id: String, value: String) -> Result<bool> {
    let n = writer
        .execute(move |c| {
            Ok(c.execute(
                "UPDATE credentials SET value = ?1 WHERE id = ?2",
                params![value, id],
            )?)
        })
        .await?;
    Ok(n > 0)
}

pub fn get(conn: &Connection, id: &str) -> Result<Option<Credential>> {
    let mut stmt = conn.prepare(
        "SELECT id, provider, kind, value, label, created_at
         FROM credentials
         WHERE id = ?1",
    )?;
    let mut rows = stmt.query(params![id])?;
    if let Some(r) = rows.next()? {
        Ok(Some(Credential {
            id: r.get(0)?,
            provider: r.get(1)?,
            kind: r.get(2)?,
            value: r.get(3)?,
            label: r.get(4)?,
            created_at: r.get(5)?,
        }))
    } else {
        Ok(None)
    }
}

pub fn list_for_provider(conn: &Connection, provider: &str) -> Result<Vec<Credential>> {
    let mut stmt = conn.prepare(
        "SELECT id, provider, kind, value, label, created_at
         FROM credentials
         WHERE provider = ?1
         ORDER BY created_at",
    )?;
    let iter = stmt.query_map(params![provider], |r| {
        Ok(Credential {
            id: r.get(0)?,
            provider: r.get(1)?,
            kind: r.get(2)?,
            value: r.get(3)?,
            label: r.get(4)?,
            created_at: r.get(5)?,
        })
    })?;
    let mut out = Vec::new();
    for row in iter {
        out.push(row?);
    }
    Ok(out)
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
    async fn insert_then_list() {
        let (f, w) = setup();
        insert(
            &w,
            "anthropic".into(),
            "api_key".into(),
            "sk-test".into(),
            Some("personal".into()),
        )
        .await
        .unwrap();
        let c = Connection::open(f.path()).unwrap();
        let all = list(&c).unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].provider, "anthropic");
        assert_eq!(all[0].value, "sk-test");
    }

    #[tokio::test]
    async fn delete_removes() {
        let (f, w) = setup();
        let id = insert(&w, "openai".into(), "api_key".into(), "sk-y".into(), None)
            .await
            .unwrap();
        assert!(delete(&w, id).await.unwrap());
        let c = Connection::open(f.path()).unwrap();
        assert!(list(&c).unwrap().is_empty());
    }

    #[tokio::test]
    async fn list_for_provider_filters() {
        let (f, w) = setup();
        insert(&w, "anthropic".into(), "api_key".into(), "a".into(), None)
            .await
            .unwrap();
        insert(&w, "openai".into(), "api_key".into(), "o".into(), None)
            .await
            .unwrap();
        insert(&w, "anthropic".into(), "api_key".into(), "a2".into(), None)
            .await
            .unwrap();
        let c = Connection::open(f.path()).unwrap();
        let ant = list_for_provider(&c, "anthropic").unwrap();
        assert_eq!(ant.len(), 2);
    }
}
