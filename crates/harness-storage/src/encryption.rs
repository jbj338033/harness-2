// IMPLEMENTS: D-211
//! At-rest encryption probe. If the linked SQLite was built with the
//! SQLCipher extension we surface that capability so the daemon can
//! transparently turn it on; if not we emit a single warning and run
//! against a plain database. D-211 marks the warning path "launch
//! mandatory" — the daemon must never silently accept a downgrade.

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EncryptionStatus {
    /// SQLCipher detected — at-rest encryption is available.
    Available,
    /// Plain SQLite — operator must accept the warning before launch.
    PlainTextOnly,
}

#[must_use]
pub fn detect(conn: &Connection) -> EncryptionStatus {
    // SQLCipher exposes a `cipher_version` pragma even before a key is
    // set. Plain SQLite returns no rows.
    let version: rusqlite::Result<String> =
        conn.pragma_query_value(None, "cipher_version", |row| row.get(0));
    match version {
        Ok(v) if !v.is_empty() => EncryptionStatus::Available,
        _ => EncryptionStatus::PlainTextOnly,
    }
}

/// Apply the SQLCipher key. Returns Err on plain SQLite — caller decides
/// whether to bail or carry on with the warning.
pub fn apply_key(conn: &Connection, key: &str) -> Result<(), crate::StorageError> {
    if detect(conn) != EncryptionStatus::Available {
        return Err(crate::StorageError::Invariant(
            "sqlcipher not linked; cannot apply key".into(),
        ));
    }
    conn.pragma_update(None, "key", key)?;
    Ok(())
}

/// Single canonical warning string the daemon logs at boot when running
/// against plain SQLite.
pub const PLAINTEXT_WARNING: &str = "harness.db is plaintext (sqlcipher not detected). At-rest secrets are protected by file mode only. Re-run after installing a sqlcipher build to encrypt.";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vanilla_sqlite_reports_plaintext() {
        let conn = Connection::open_in_memory().unwrap();
        assert_eq!(detect(&conn), EncryptionStatus::PlainTextOnly);
    }

    #[test]
    fn apply_key_refuses_on_plain_build() {
        let conn = Connection::open_in_memory().unwrap();
        assert!(apply_key(&conn, "anything").is_err());
    }

    #[test]
    fn warning_text_is_actionable() {
        assert!(PLAINTEXT_WARNING.contains("sqlcipher"));
        assert!(PLAINTEXT_WARNING.contains("plaintext"));
    }
}
