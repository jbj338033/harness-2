// IMPLEMENTS: D-305
//! Periodic 8-axis memory scan schedule. Successor to D-186 — every
//! `NEXT_SCAN_INTERVAL_HOURS` we re-scan the long-term memory store
//! across the eight bias axes (race / gender / age / disability /
//! religion / nationality / sexuality / socio-economic).

use serde::{Deserialize, Serialize};

pub const NEXT_SCAN_INTERVAL_HOURS: i64 = 24 * 7;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryScanSchedule {
    pub last_scan_ms: Option<i64>,
}

#[must_use]
pub fn due_at_ms(schedule: MemoryScanSchedule, now_ms: i64) -> bool {
    match schedule.last_scan_ms {
        None => true,
        Some(last) => {
            let interval_ms: i64 = NEXT_SCAN_INTERVAL_HOURS * 60 * 60 * 1000;
            now_ms.saturating_sub(last) >= interval_ms
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn never_scanned_is_due() {
        assert!(due_at_ms(MemoryScanSchedule { last_scan_ms: None }, 0));
    }

    #[test]
    fn fresh_scan_not_due() {
        let s = MemoryScanSchedule {
            last_scan_ms: Some(1_000_000_000),
        };
        assert!(!due_at_ms(s, 1_000_001_000));
    }

    #[test]
    fn after_one_week_due() {
        let one_week_ms: i64 = NEXT_SCAN_INTERVAL_HOURS * 60 * 60 * 1000;
        let s = MemoryScanSchedule {
            last_scan_ms: Some(0),
        };
        assert!(due_at_ms(s, one_week_ms));
    }
}
