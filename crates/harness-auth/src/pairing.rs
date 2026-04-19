use crate::key::PublicKey;
use harness_core::now;
use harness_storage::WriterHandle;
use rand::Rng;
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use thiserror::Error;

pub const DEFAULT_TTL: Duration = Duration::from_secs(300);

#[derive(Debug, Error)]
pub enum PairingError {
    #[error("pairing code is invalid or expired")]
    Invalid,
    #[error("public key already paired")]
    AlreadyPaired,
    #[error("storage: {0}")]
    Storage(#[from] harness_storage::StorageError),
}

#[derive(Clone)]
pub struct PairingSession {
    codes: Arc<Mutex<HashMap<String, Pending>>>,
    ttl: Duration,
}

#[derive(Clone)]
struct Pending {
    expires_at: Instant,
}

impl Default for PairingSession {
    fn default() -> Self {
        Self::new(DEFAULT_TTL)
    }
}

impl PairingSession {
    #[must_use]
    pub fn new(ttl: Duration) -> Self {
        Self {
            codes: Arc::new(Mutex::new(HashMap::new())),
            ttl,
        }
    }

    #[must_use]
    pub fn new_code(&self) -> String {
        let code = random_code();
        let mut map = self.codes.lock().expect("mutex poisoned");
        map.insert(
            code.clone(),
            Pending {
                expires_at: Instant::now() + self.ttl,
            },
        );
        Self::gc(&mut map);
        code
    }

    pub fn consume(&self, code: &str) -> Result<(), PairingError> {
        let mut map = self.codes.lock().expect("mutex poisoned");
        Self::gc(&mut map);
        match map.remove(code) {
            Some(p) if p.expires_at > Instant::now() => Ok(()),
            _ => Err(PairingError::Invalid),
        }
    }

    fn gc(map: &mut HashMap<String, Pending>) {
        let now = Instant::now();
        map.retain(|_, p| p.expires_at > now);
    }

    #[must_use]
    pub fn live_codes(&self) -> usize {
        let mut map = self.codes.lock().expect("mutex poisoned");
        Self::gc(&mut map);
        map.len()
    }
}

fn random_code() -> String {
    const CHARS: &[u8] = b"abcdefghjkmnpqrstuvwxyz23456789";
    let mut rng = rand::thread_rng();
    let mut out = String::with_capacity(9);
    for (i, _) in (0..8).enumerate() {
        if i == 4 {
            out.push('-');
        }
        out.push(char::from(CHARS[rng.gen_range(0..CHARS.len())]));
    }
    out
}

pub fn consume_code(session: &PairingSession, code: &str) -> Result<(), PairingError> {
    session.consume(code)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DeviceRecord {
    pub id: String,
    pub name: String,
    pub public_key: PublicKey,
    pub last_seen_at: Option<i64>,
    pub created_at: i64,
}

pub async fn register_device(
    writer: &WriterHandle,
    name: String,
    public_key: PublicKey,
) -> Result<DeviceRecord, PairingError> {
    let id = uuid::Uuid::new_v4().to_string();
    let ts = now().as_millis();
    let pk_bytes = public_key.0.to_vec();
    let id_clone = id.clone();
    let name_clone = name.clone();

    writer
        .execute(move |c| {
            let mut stmt = c.prepare("SELECT id FROM devices WHERE public_key = ?1")?;
            let mut rows = stmt.query(params![pk_bytes])?;
            if rows.next()?.is_some() {
                return Err(harness_storage::StorageError::Invariant(
                    "public key already paired".into(),
                ));
            }
            c.execute(
                "INSERT INTO devices (id, name, public_key, last_seen_at, created_at)
                 VALUES (?1, ?2, ?3, NULL, ?4)",
                params![id_clone, name_clone, pk_bytes, ts],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| match e {
            harness_storage::StorageError::Invariant(_) => PairingError::AlreadyPaired,
            other => PairingError::Storage(other),
        })?;

    Ok(DeviceRecord {
        id,
        name,
        public_key,
        last_seen_at: None,
        created_at: ts,
    })
}

pub async fn revoke_device(writer: &WriterHandle, id: String) -> Result<bool, PairingError> {
    let n = writer
        .execute(move |c| Ok(c.execute("DELETE FROM devices WHERE id = ?1", params![id])?))
        .await?;
    Ok(n > 0)
}

pub fn list_devices(conn: &Connection) -> Result<Vec<DeviceRecord>, PairingError> {
    let mut stmt = conn.prepare(
        "SELECT id, name, public_key, last_seen_at, created_at
         FROM devices
         ORDER BY created_at DESC",
    )?;
    let iter = stmt.query_map([], |row| {
        let id: String = row.get(0)?;
        let name: String = row.get(1)?;
        let pk_bytes: Vec<u8> = row.get(2)?;
        let last_seen_at: Option<i64> = row.get(3)?;
        let created_at: i64 = row.get(4)?;
        if pk_bytes.len() != 32 {
            return Err(rusqlite::Error::FromSqlConversionFailure(
                2,
                rusqlite::types::Type::Blob,
                Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "public key must be 32 bytes",
                )),
            ));
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&pk_bytes);
        Ok(DeviceRecord {
            id,
            name,
            public_key: PublicKey(arr),
            last_seen_at,
            created_at,
        })
    })?;
    let mut out = Vec::new();
    for r in iter {
        out.push(r?);
    }
    Ok(out)
}

impl From<rusqlite::Error> for PairingError {
    fn from(e: rusqlite::Error) -> Self {
        PairingError::Storage(harness_storage::StorageError::Sqlite(e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::key::generate_keypair;
    use harness_storage::{Database, Writer};
    use tempfile::NamedTempFile;

    fn setup() -> (NamedTempFile, WriterHandle) {
        let f = NamedTempFile::new().unwrap();
        Database::open(f.path()).unwrap();
        let h = Writer::spawn(f.path()).unwrap();
        (f, h)
    }

    #[test]
    fn code_format_is_human_friendly() {
        let s = PairingSession::default();
        let code = s.new_code();
        assert_eq!(code.len(), 9);
        assert_eq!(code.chars().filter(|&c| c == '-').count(), 1);
    }

    #[test]
    fn consume_invalidates() {
        let s = PairingSession::default();
        let code = s.new_code();
        assert!(s.consume(&code).is_ok());
        assert!(matches!(s.consume(&code), Err(PairingError::Invalid)));
    }

    #[test]
    fn expired_code_is_rejected() {
        let s = PairingSession::new(Duration::from_millis(1));
        let code = s.new_code();
        std::thread::sleep(Duration::from_millis(10));
        assert!(matches!(s.consume(&code), Err(PairingError::Invalid)));
    }

    #[tokio::test]
    async fn register_persists() {
        let (f, w) = setup();
        let (_, pk) = generate_keypair();
        let dev = register_device(&w, "phone".into(), pk.clone())
            .await
            .unwrap();
        assert_eq!(dev.name, "phone");

        let reader = rusqlite::Connection::open(f.path()).unwrap();
        let devices = list_devices(&reader).unwrap();
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].public_key, pk);
    }

    #[tokio::test]
    async fn duplicate_key_rejected() {
        let (_f, w) = setup();
        let (_, pk) = generate_keypair();
        register_device(&w, "a".into(), pk.clone()).await.unwrap();
        let err = register_device(&w, "b".into(), pk).await.unwrap_err();
        assert!(matches!(err, PairingError::AlreadyPaired));
    }

    #[tokio::test]
    async fn revoke_device_removes() {
        let (f, w) = setup();
        let (_, pk) = generate_keypair();
        let dev = register_device(&w, "phone".into(), pk).await.unwrap();
        assert!(revoke_device(&w, dev.id.clone()).await.unwrap());

        let reader = rusqlite::Connection::open(f.path()).unwrap();
        assert!(list_devices(&reader).unwrap().is_empty());
    }

    #[test]
    fn gc_drops_expired_codes() {
        let s = PairingSession::new(Duration::from_millis(1));
        let a = s.new_code();
        let b = s.new_code();
        assert_ne!(a, b);
        assert_eq!(s.live_codes(), 2);
        std::thread::sleep(Duration::from_millis(10));
        assert_eq!(s.live_codes(), 0);
    }
}
