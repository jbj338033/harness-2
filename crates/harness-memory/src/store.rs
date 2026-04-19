use crate::{MemoryError, Result};
use harness_core::now;
use harness_storage::WriterHandle;
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Scope {
    Global,
    Project(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MemoryRecord {
    pub id: String,
    pub scope: Scope,
    pub content: String,
    pub created_at: i64,
}

#[derive(Debug, Clone)]
pub struct NewMemory {
    pub scope: Scope,
    pub content: String,
}

pub async fn insert(writer: &WriterHandle, new: NewMemory) -> Result<String> {
    if new.content.trim().is_empty() {
        return Err(MemoryError::Input("empty content".into()));
    }
    let id = uuid::Uuid::new_v4().to_string();
    let ts = now().as_millis();
    let project = match &new.scope {
        Scope::Global => None,
        Scope::Project(p) => Some(p.clone()),
    };
    let content = new.content;
    let id_clone = id.clone();
    writer
        .execute(move |c| {
            c.execute(
                "INSERT INTO memory (id, project, content, created_at) VALUES (?1, ?2, ?3, ?4)",
                params![id_clone, project, content, ts],
            )?;
            Ok(())
        })
        .await?;
    Ok(id)
}

pub async fn delete(writer: &WriterHandle, id: String) -> Result<bool> {
    let affected = writer
        .execute(move |c| {
            let n = c.execute("DELETE FROM memory WHERE id = ?1", params![id])?;
            Ok(n)
        })
        .await?;
    Ok(affected > 0)
}

pub fn list_global(conn: &Connection) -> Result<Vec<MemoryRecord>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, project, content, created_at
             FROM memory
             WHERE project IS NULL
             ORDER BY created_at DESC",
        )
        .map_err(|e| MemoryError::Storage(harness_storage::StorageError::Sqlite(e)))?;
    collect(&mut stmt, params![])
}

pub fn list_project(conn: &Connection, cwd: &str) -> Result<Vec<MemoryRecord>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, project, content, created_at
             FROM memory
             WHERE project IS NOT NULL
             ORDER BY length(project) DESC, created_at DESC",
        )
        .map_err(|e| MemoryError::Storage(harness_storage::StorageError::Sqlite(e)))?;
    let all: Vec<MemoryRecord> = collect(&mut stmt, params![])?;
    Ok(all
        .into_iter()
        .filter(|m| match &m.scope {
            Scope::Project(p) => cwd == p || cwd.starts_with(&format!("{p}/")),
            Scope::Global => false,
        })
        .collect())
}

fn collect<P: rusqlite::Params>(
    stmt: &mut rusqlite::Statement<'_>,
    p: P,
) -> Result<Vec<MemoryRecord>> {
    let iter = stmt
        .query_map(p, row_to_record)
        .map_err(|e| MemoryError::Storage(harness_storage::StorageError::Sqlite(e)))?;
    let mut out = Vec::new();
    for r in iter {
        out.push(r.map_err(|e| MemoryError::Storage(harness_storage::StorageError::Sqlite(e)))?);
    }
    Ok(out)
}

fn row_to_record(row: &rusqlite::Row) -> rusqlite::Result<MemoryRecord> {
    let id: String = row.get(0)?;
    let project: Option<String> = row.get(1)?;
    let content: String = row.get(2)?;
    let created_at: i64 = row.get(3)?;
    let scope = match project {
        Some(p) => Scope::Project(p),
        None => Scope::Global,
    };
    Ok(MemoryRecord {
        id,
        scope,
        content,
        created_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use harness_storage::{Database, Writer};
    use tempfile::NamedTempFile;

    fn setup() -> (NamedTempFile, WriterHandle) {
        let f = NamedTempFile::new().unwrap();
        Database::open(f.path()).unwrap();
        let h = Writer::spawn(f.path()).unwrap();
        (f, h)
    }

    #[tokio::test]
    async fn insert_global_and_list() {
        let (f, w) = setup();
        insert(
            &w,
            NewMemory {
                scope: Scope::Global,
                content: "use pnpm".into(),
            },
        )
        .await
        .unwrap();

        let reader = rusqlite::Connection::open(f.path()).unwrap();
        let all = list_global(&reader).unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].content, "use pnpm");
    }

    #[tokio::test]
    async fn project_memories_prefix_match() {
        let (f, w) = setup();
        insert(
            &w,
            NewMemory {
                scope: Scope::Project("/work/proj".into()),
                content: "rust + axum".into(),
            },
        )
        .await
        .unwrap();
        insert(
            &w,
            NewMemory {
                scope: Scope::Project("/work/other".into()),
                content: "python".into(),
            },
        )
        .await
        .unwrap();
        insert(
            &w,
            NewMemory {
                scope: Scope::Global,
                content: "korean".into(),
            },
        )
        .await
        .unwrap();

        let reader = rusqlite::Connection::open(f.path()).unwrap();
        let hits = list_project(&reader, "/work/proj/sub").unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].content, "rust + axum");
    }

    #[tokio::test]
    async fn delete_removes_row() {
        let (f, w) = setup();
        let id = insert(
            &w,
            NewMemory {
                scope: Scope::Global,
                content: "x".into(),
            },
        )
        .await
        .unwrap();
        assert!(delete(&w, id).await.unwrap());

        let reader = rusqlite::Connection::open(f.path()).unwrap();
        assert!(list_global(&reader).unwrap().is_empty());
    }

    #[tokio::test]
    async fn empty_content_rejected() {
        let (_f, w) = setup();
        let err = insert(
            &w,
            NewMemory {
                scope: Scope::Global,
                content: "   ".into(),
            },
        )
        .await
        .unwrap_err();
        assert!(matches!(err, MemoryError::Input(_)));
    }

    #[tokio::test]
    async fn specific_project_beats_parent() {
        let (f, w) = setup();
        insert(
            &w,
            NewMemory {
                scope: Scope::Project("/work".into()),
                content: "parent".into(),
            },
        )
        .await
        .unwrap();
        insert(
            &w,
            NewMemory {
                scope: Scope::Project("/work/proj".into()),
                content: "child".into(),
            },
        )
        .await
        .unwrap();

        let reader = rusqlite::Connection::open(f.path()).unwrap();
        let hits = list_project(&reader, "/work/proj").unwrap();
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].content, "child");
        assert_eq!(hits[1].content, "parent");
    }
}
