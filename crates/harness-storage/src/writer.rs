// IMPLEMENTS: D-075
use crate::{Result, StorageError, db};
use rusqlite::{Connection, Transaction};
use std::path::Path;
use tokio::sync::{mpsc, oneshot};

pub(crate) type WriteFn =
    Box<dyn for<'a> FnOnce(&'a mut Connection) -> Result<Box<dyn std::any::Any + Send>> + Send>;

struct WriteOp {
    run: WriteFn,
    reply: oneshot::Sender<Result<Box<dyn std::any::Any + Send>>>,
}

#[derive(Clone)]
pub struct WriterHandle {
    tx: mpsc::Sender<WriteOp>,
}

impl WriterHandle {
    pub async fn execute<F, T>(&self, f: F) -> Result<T>
    where
        F: for<'a> FnOnce(&'a mut Connection) -> Result<T> + Send + 'static,
        T: Send + 'static,
    {
        let (reply_tx, reply_rx) = oneshot::channel();
        let boxed: WriteFn = Box::new(move |conn| {
            let out = f(conn)?;
            Ok(Box::new(out) as Box<dyn std::any::Any + Send>)
        });

        self.tx
            .send(WriteOp {
                run: boxed,
                reply: reply_tx,
            })
            .await
            .map_err(|_| StorageError::WriterUnavailable)?;

        let any = reply_rx
            .await
            .map_err(|_| StorageError::WriterUnavailable)??;
        Ok(*any
            .downcast::<T>()
            .expect("writer closure returned unexpected type"))
    }

    pub async fn with_tx<F, T>(&self, f: F) -> Result<T>
    where
        F: for<'a> FnOnce(&'a Transaction<'a>) -> Result<T> + Send + 'static,
        T: Send + 'static,
    {
        self.execute(move |conn| {
            let tx = conn.transaction()?;
            let value = f(&tx)?;
            tx.commit()?;
            Ok(value)
        })
        .await
    }
}

pub struct Writer;

impl Writer {
    pub fn spawn(db_path: &Path) -> Result<WriterHandle> {
        let (tx, mut rx) = mpsc::channel::<WriteOp>(128);

        let mut conn = Connection::open(db_path)?;
        db::configure(&mut conn)?;

        std::thread::Builder::new()
            .name("harness-storage-writer".into())
            .spawn(move || {
                while let Some(op) = rx.blocking_recv() {
                    let result = (op.run)(&mut conn);
                    op.reply.send(result).ok();
                }
                tracing::debug!("harness-storage writer task exiting");
            })?;

        Ok(WriterHandle { tx })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::migrations;
    use tempfile::NamedTempFile;

    fn setup() -> (NamedTempFile, WriterHandle) {
        let f = NamedTempFile::new().unwrap();
        let mut conn = Connection::open(f.path()).unwrap();
        db::configure(&mut conn).unwrap();
        migrations::apply(&mut conn).unwrap();
        drop(conn);

        let handle = Writer::spawn(f.path()).unwrap();
        (f, handle)
    }

    #[tokio::test]
    async fn write_then_read() {
        let (_f, h) = setup();
        let inserted: i64 = h
            .execute(|conn| {
                conn.execute(
                    "INSERT INTO config (key, value) VALUES ('test', 'value')",
                    [],
                )?;
                Ok(conn.last_insert_rowid())
            })
            .await
            .unwrap();
        assert!(inserted > 0);

        let value: String = h
            .execute(|conn| {
                Ok(
                    conn.query_row("SELECT value FROM config WHERE key = 'test'", [], |row| {
                        row.get(0)
                    })?,
                )
            })
            .await
            .unwrap();
        assert_eq!(value, "value");
    }

    #[tokio::test]
    async fn concurrent_writes_serialize() {
        let (_f, h) = setup();
        let mut tasks = Vec::new();
        for i in 0..20 {
            let h2 = h.clone();
            tasks.push(tokio::spawn(async move {
                h2.execute(move |conn| {
                    conn.execute(
                        "INSERT INTO config (key, value) VALUES (?1, ?2)",
                        rusqlite::params![format!("k{i}"), format!("v{i}")],
                    )?;
                    Ok(())
                })
                .await
            }));
        }
        for t in tasks {
            t.await.unwrap().unwrap();
        }

        let count: i64 = h
            .execute(
                |conn| Ok(conn.query_row("SELECT COUNT(*) FROM config", [], |row| row.get(0))?),
            )
            .await
            .unwrap();
        assert_eq!(count, 20);
    }

    #[tokio::test]
    async fn error_propagates() {
        let (_f, h) = setup();
        let err = h
            .execute(|conn| {
                conn.execute("THIS IS NOT SQL", [])?;
                Ok(())
            })
            .await
            .unwrap_err();
        assert!(matches!(err, StorageError::Sqlite(_)));
    }

    #[tokio::test]
    async fn with_tx_commits_on_success() {
        let (_f, h) = setup();
        h.with_tx(|tx| {
            tx.execute("INSERT INTO config (key, value) VALUES ('a', '1')", [])?;
            tx.execute("INSERT INTO config (key, value) VALUES ('b', '2')", [])?;
            Ok(())
        })
        .await
        .unwrap();

        let count: i64 = h
            .execute(
                |conn| Ok(conn.query_row("SELECT COUNT(*) FROM config", [], |row| row.get(0))?),
            )
            .await
            .unwrap();
        assert_eq!(count, 2);
    }

    #[tokio::test]
    async fn with_tx_rolls_back_when_closure_errors() {
        let (_f, h) = setup();
        let err = h
            .with_tx(|tx| -> Result<()> {
                tx.execute("INSERT INTO config (key, value) VALUES ('a', '1')", [])?;
                Err(StorageError::Invariant("rollback please".into()))
            })
            .await
            .unwrap_err();
        assert!(matches!(err, StorageError::Invariant(_)));

        let count: i64 = h
            .execute(
                |conn| Ok(conn.query_row("SELECT COUNT(*) FROM config", [], |row| row.get(0))?),
            )
            .await
            .unwrap();
        assert_eq!(count, 0, "failed transaction must leave no rows");
    }

    #[tokio::test]
    async fn with_tx_rolls_back_when_sql_errors_midway() {
        let (_f, h) = setup();
        let err = h
            .with_tx(|tx| {
                tx.execute("INSERT INTO config (key, value) VALUES ('a', '1')", [])?;
                tx.execute("THIS IS NOT SQL", [])?;
                Ok(())
            })
            .await
            .unwrap_err();
        assert!(matches!(err, StorageError::Sqlite(_)));

        let count: i64 = h
            .execute(
                |conn| Ok(conn.query_row("SELECT COUNT(*) FROM config", [], |row| row.get(0))?),
            )
            .await
            .unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn with_tx_returns_value() {
        let (_f, h) = setup();
        let n = h
            .with_tx(|tx| {
                tx.execute("INSERT INTO config (key, value) VALUES ('k', 'v')", [])?;
                Ok(tx.last_insert_rowid())
            })
            .await
            .unwrap();
        assert!(n > 0);
    }
}
