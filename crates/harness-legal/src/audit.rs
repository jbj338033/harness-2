// IMPLEMENTS: D-359
//! Per-matter append-only ledger with a blake3 hash chain.
//!
//! Each entry's `prev` is the hash of the previous entry (or zeroes
//! for the genesis row). Tampering breaks the link, which lets a
//! later FOIA-style audit detect the edit. Companion to D-387's
//! gov-side ledger.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditEntry {
    pub seq: u64,
    pub prev: [u8; 32],
    pub kind: String,
    pub body: serde_json::Value,
    pub at_ms: i64,
}

impl AuditEntry {
    /// Hash includes seq + prev + kind + body + at_ms — every field.
    #[must_use]
    pub fn hash(&self) -> [u8; 32] {
        let mut h = blake3::Hasher::new();
        h.update(b"harness/legal/audit/v1\n");
        h.update(&self.seq.to_le_bytes());
        h.update(&self.prev);
        h.update(self.kind.as_bytes());
        let body_bytes =
            serde_json::to_vec(&self.body).unwrap_or_else(|_| b"<unserialisable>".to_vec());
        h.update(&body_bytes);
        h.update(&self.at_ms.to_le_bytes());
        *h.finalize().as_bytes()
    }
}

#[derive(Debug, Default)]
pub struct MatterLedger {
    entries: Vec<AuditEntry>,
}

impl MatterLedger {
    pub fn append(&mut self, kind: impl Into<String>, body: serde_json::Value, at_ms: i64) {
        let prev = self.entries.last().map_or([0u8; 32], AuditEntry::hash);
        let seq = u64::try_from(self.entries.len()).unwrap_or(u64::MAX);
        self.entries.push(AuditEntry {
            seq,
            prev,
            kind: kind.into(),
            body,
            at_ms,
        });
    }

    #[must_use]
    pub fn entries(&self) -> &[AuditEntry] {
        &self.entries
    }

    /// Walk the chain — returns Err on the first link that doesn't
    /// match its predecessor's hash.
    pub fn verify(&self) -> Result<(), u64> {
        for (i, entry) in self.entries.iter().enumerate() {
            let expected_prev = if i == 0 {
                [0u8; 32]
            } else {
                self.entries[i - 1].hash()
            };
            if entry.prev != expected_prev {
                return Err(entry.seq);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_ledger_verifies() {
        let mut l = MatterLedger::default();
        l.append("intake", serde_json::json!({"client": "Acme"}), 1);
        l.append("draft", serde_json::json!({"doc": "complaint"}), 2);
        assert!(l.verify().is_ok());
        assert_eq!(l.entries().len(), 2);
    }

    #[test]
    fn tampered_body_breaks_chain() {
        let mut l = MatterLedger::default();
        l.append("intake", serde_json::json!({"v": 1}), 1);
        l.append("draft", serde_json::json!({"v": 2}), 2);
        // Tamper with the genesis entry's body — the second entry's
        // `prev` no longer matches the new hash of entry 0.
        l.entries[0].body = serde_json::json!({"v": 99});
        assert_eq!(l.verify(), Err(1));
    }

    #[test]
    fn genesis_prev_is_all_zero() {
        let mut l = MatterLedger::default();
        l.append("g", serde_json::json!({}), 0);
        assert_eq!(l.entries[0].prev, [0u8; 32]);
    }
}
