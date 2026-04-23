// IMPLEMENTS: D-128
//! 5-signal corrupted-root detector. When recovering from a crash
//! the daemon must decide whether the candidate root event is real
//! or a torn write. We treat the root as accepted when at least 4
//! of the 5 signals hold; otherwise we surface a clarification
//! question so the operator picks. (Replaces D-114's simpler
//! "monotonic seq" heuristic.)

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CorruptedRootSignals {
    /// Sequence number is the smallest in the candidate set.
    pub seq_is_min: bool,
    /// Parent event id resolves in the events table.
    pub parent_exists: bool,
    /// Session id matches the recovered session header.
    pub session_matches: bool,
    /// Timestamp is monotonically ahead of the prior write.
    pub timestamp_monotonic: bool,
    /// Body deserialises cleanly against the schema.
    pub body_deserialises: bool,
}

impl CorruptedRootSignals {
    #[must_use]
    pub fn count(self) -> u8 {
        u8::from(self.seq_is_min)
            + u8::from(self.parent_exists)
            + u8::from(self.session_matches)
            + u8::from(self.timestamp_monotonic)
            + u8::from(self.body_deserialises)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CorruptedRootVerdict {
    /// 4 or 5 signals matched — accept the root.
    Accept,
    /// Below 4 — emit `Speak(ClarificationQuestion)` so the operator
    /// disambiguates.
    AskClarification,
}

#[must_use]
pub fn classify_root(signals: CorruptedRootSignals) -> CorruptedRootVerdict {
    if signals.count() >= 4 {
        CorruptedRootVerdict::Accept
    } else {
        CorruptedRootVerdict::AskClarification
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(a: bool, b: bool, c: bool, d: bool, e: bool) -> CorruptedRootSignals {
        CorruptedRootSignals {
            seq_is_min: a,
            parent_exists: b,
            session_matches: c,
            timestamp_monotonic: d,
            body_deserialises: e,
        }
    }

    #[test]
    fn five_signals_accept() {
        assert_eq!(
            classify_root(s(true, true, true, true, true)),
            CorruptedRootVerdict::Accept
        );
    }

    #[test]
    fn four_signals_accept() {
        assert_eq!(
            classify_root(s(true, true, true, true, false)),
            CorruptedRootVerdict::Accept
        );
    }

    #[test]
    fn three_signals_ask() {
        assert_eq!(
            classify_root(s(true, true, true, false, false)),
            CorruptedRootVerdict::AskClarification
        );
    }

    #[test]
    fn no_signals_ask() {
        assert_eq!(
            classify_root(s(false, false, false, false, false)),
            CorruptedRootVerdict::AskClarification
        );
    }
}
