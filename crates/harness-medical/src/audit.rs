// IMPLEMENTS: D-365
//! PHI-touch audit ledger with a 6-year HITECH retention window.
//! Anything that classifies as a PHI touch (read, write, summarise,
//! send-to-provider) lands in this ledger.

use crate::phi::PhiClass;
use serde::{Deserialize, Serialize};

pub const RETENTION_YEARS: u32 = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PhiTouchKind {
    Read,
    Write,
    Summarise,
    SentToProvider,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PhiAuditEntry {
    pub at_ms: i64,
    pub actor_id: String,
    pub kind: PhiTouchKind,
    pub classes: Vec<PhiClass>,
    pub note: String,
}

#[derive(Debug, Default)]
pub struct PhiTouchLedger {
    entries: Vec<PhiAuditEntry>,
}

impl PhiTouchLedger {
    pub fn record(&mut self, entry: PhiAuditEntry) {
        self.entries.push(entry);
    }

    #[must_use]
    pub fn entries(&self) -> &[PhiAuditEntry] {
        &self.entries
    }

    /// Drop entries older than [`RETENTION_YEARS`] relative to `now_ms`.
    /// Returns the number of dropped entries. The 6-year window is the
    /// HITECH minimum; institutions may set a longer one but never
    /// shorter.
    pub fn purge_expired(&mut self, now_ms: i64) -> usize {
        let cutoff = now_ms.saturating_sub(i64::from(RETENTION_YEARS) * 365 * 24 * 60 * 60 * 1000);
        let before = self.entries.len();
        self.entries.retain(|e| e.at_ms >= cutoff);
        before - self.entries.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(at_ms: i64) -> PhiAuditEntry {
        PhiAuditEntry {
            at_ms,
            actor_id: "actor-1".into(),
            kind: PhiTouchKind::Read,
            classes: vec![PhiClass::Mrn],
            note: "chart".into(),
        }
    }

    #[test]
    fn record_appends() {
        let mut l = PhiTouchLedger::default();
        l.record(entry(1));
        l.record(entry(2));
        assert_eq!(l.entries().len(), 2);
    }

    #[test]
    fn purge_drops_only_expired() {
        let mut l = PhiTouchLedger::default();
        let day_ms: i64 = 24 * 60 * 60 * 1000;
        let year_ms: i64 = 365 * day_ms;
        l.record(entry(0));
        l.record(entry(7 * year_ms));
        let now = 8 * year_ms;
        let dropped = l.purge_expired(now);
        assert_eq!(dropped, 1);
        assert_eq!(l.entries().len(), 1);
    }
}
