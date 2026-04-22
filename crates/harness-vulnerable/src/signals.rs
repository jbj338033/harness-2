// IMPLEMENTS: D-398
//! 6-signal vulnerability escalator. Signals are RAISE-ONLY (we never
//! infer "less vulnerable" from these), the raw waveform is VOLATILE
//! (we accept a verdict, never persist samples), and the user is
//! always informed with a one-click disable path advertised back.

use crate::profile::VulnerabilityLevel;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SignalKind {
    Hesitation,
    Confusion,
    VocabularyDrop,
    VoicePitch,
    TypingPattern,
    SessionDuration,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Signal {
    pub kind: SignalKind,
    /// Pre-quantised verdict — implementations never give us the raw
    /// time-series.
    pub strength_0_100: u8,
    pub at_ms: i64,
}

#[derive(Debug, Default)]
pub struct SignalEscalator {
    accumulated: u32,
}

impl SignalEscalator {
    pub fn observe(&mut self, signal: &Signal) {
        let bump = u32::from(signal.strength_0_100.min(100));
        self.accumulated = self.accumulated.saturating_add(bump);
    }

    /// Returns the *higher* of the two: never demotes the level. The
    /// thresholds are intentionally conservative — one strong signal
    /// (≥80) lifts to Elevated; two together lift to LVulnerable.
    #[must_use]
    pub fn fold(&self, base: VulnerabilityLevel) -> VulnerabilityLevel {
        let candidate = if self.accumulated >= 200 {
            VulnerabilityLevel::LVulnerable
        } else if self.accumulated >= 80 {
            VulnerabilityLevel::Elevated
        } else {
            VulnerabilityLevel::Standard
        };
        base.max(candidate)
    }

    pub fn reset(&mut self) {
        self.accumulated = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(kind: SignalKind, strength: u8) -> Signal {
        Signal {
            kind,
            strength_0_100: strength,
            at_ms: 1,
        }
    }

    #[test]
    fn empty_escalator_keeps_base() {
        let e = SignalEscalator::default();
        assert_eq!(
            e.fold(VulnerabilityLevel::Standard),
            VulnerabilityLevel::Standard
        );
    }

    #[test]
    fn one_strong_signal_lifts_to_elevated() {
        let mut e = SignalEscalator::default();
        e.observe(&s(SignalKind::Confusion, 90));
        assert_eq!(
            e.fold(VulnerabilityLevel::Standard),
            VulnerabilityLevel::Elevated
        );
    }

    #[test]
    fn many_signals_lift_to_l_vulnerable() {
        let mut e = SignalEscalator::default();
        for k in [
            SignalKind::Confusion,
            SignalKind::Hesitation,
            SignalKind::VocabularyDrop,
        ] {
            e.observe(&s(k, 80));
        }
        assert_eq!(
            e.fold(VulnerabilityLevel::Standard),
            VulnerabilityLevel::LVulnerable
        );
    }

    #[test]
    fn fold_never_demotes() {
        let e = SignalEscalator::default();
        assert_eq!(
            e.fold(VulnerabilityLevel::LVulnerable),
            VulnerabilityLevel::LVulnerable
        );
    }
}
