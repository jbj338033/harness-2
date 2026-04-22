// IMPLEMENTS: D-286, D-287, D-288, D-289, D-290, D-291, D-292
//! Third surface client (CLI / Web / **Voice**). Pure-data layer — the
//! actual audio capture / playback lives in the binary that consumes
//! this crate. We pin every voice-specific safety policy here so the
//! daemon and any future SDK draw from the same source.

use serde::{Deserialize, Serialize};
use std::time::Duration;

// ----- D-287: latency budget + Parakeet for conversational turn-taking -----

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LatencyBudget {
    pub p50: Duration,
    pub p95: Duration,
    pub barge_in: Duration,
    pub eos_error_pct: u8,
}

pub const VOICE_LATENCY_BUDGET: LatencyBudget = LatencyBudget {
    p50: Duration::from_millis(800),
    p95: Duration::from_millis(1500),
    barge_in: Duration::from_millis(150),
    eos_error_pct: 5,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SttEngine {
    /// D-287 conversational default — Parakeet TDT for turn-taking.
    ParakeetTdt,
    /// Whisper is reserved for dictation-only usage now (D-285 → D-287
    /// scope correction).
    WhisperLargeV3Turbo,
}

// ----- D-288: voice approval policy -----

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VoiceActionClass {
    /// Read-only — automatic approval.
    Read,
    /// Mutating but easy to roll back (eg. file edit) — voice OK.
    MutateReversible,
    /// Destructive — voice **never** approves to defeat spoof + replay.
    Destructive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VoiceApprovalDecision {
    AutoAllow,
    PromptVoice,
    RefuseVoiceMustUseGui,
}

#[must_use]
pub fn voice_decision(class: VoiceActionClass) -> VoiceApprovalDecision {
    match class {
        VoiceActionClass::Read => VoiceApprovalDecision::AutoAllow,
        VoiceActionClass::MutateReversible => VoiceApprovalDecision::PromptVoice,
        VoiceActionClass::Destructive => VoiceApprovalDecision::RefuseVoiceMustUseGui,
    }
}

// ----- D-289: notification policy -----

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VoiceNotificationPolicy {
    Mute,
    Summary,
    Full,
    InterruptOnly,
}

// ----- D-290: voice session subcap -----

pub const DEFAULT_VOICE_SESSION_CAP_USD: f64 = 2.0;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct VoiceSubcap {
    pub session_cap_usd: f64,
}

impl Default for VoiceSubcap {
    fn default() -> Self {
        Self {
            session_cap_usd: DEFAULT_VOICE_SESSION_CAP_USD,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VoiceCostStatus {
    Ok,
    SoftWarn,
    HardStop,
}

#[must_use]
pub fn classify_voice_cost(cap: VoiceSubcap, used_usd: f64) -> VoiceCostStatus {
    if !used_usd.is_finite() || used_usd < 0.0 {
        return VoiceCostStatus::Ok;
    }
    if used_usd >= cap.session_cap_usd {
        return VoiceCostStatus::HardStop;
    }
    if used_usd >= cap.session_cap_usd * 0.9 {
        return VoiceCostStatus::SoftWarn;
    }
    VoiceCostStatus::Ok
}

// ----- D-291: TurnOutcome::Interrupted + voice_turn_audit -----

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnOutcome {
    Completed,
    Failed,
    /// Crash recovery + voice barge-in both produce this outcome
    /// (R24 FMEA C2-2).
    Interrupted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VoiceTurnAudit {
    pub session_id: String,
    pub turn_id: String,
    pub outcome: TurnOutcome,
    pub barge_in_at_ms: Option<i64>,
    pub stt_engine: SttEngine,
    pub started_at_ms: i64,
    pub finished_at_ms: i64,
}

impl VoiceTurnAudit {
    #[must_use]
    pub fn elapsed_ms(&self) -> i64 {
        self.finished_at_ms.saturating_sub(self.started_at_ms)
    }
}

// ----- D-292: WebSocket transport + voice_meta RPC -----

pub const VOICE_WS_NAMESPACE: &str = "session.voice";
pub const VOICE_META_RPC: &str = "v1.voice.meta";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum VoiceFrame {
    /// Streamed PCM frame from the client (16-bit mono, 16 kHz).
    AudioIn { samples: Vec<i16>, sequence: u64 },
    /// Streamed model TTS chunk going back to the client.
    AudioOut { samples: Vec<i16>, sequence: u64 },
    /// User started speaking — fired by VAD for barge-in.
    BargeIn { at_ms: i64 },
    /// End-of-speech — server can stop accepting audio for this turn.
    Eos { at_ms: i64 },
    /// Out-of-band metadata (eg. notification policy change).
    Meta {
        rpc: String,
        payload: serde_json::Value,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latency_budget_matches_d_287() {
        assert_eq!(VOICE_LATENCY_BUDGET.p50, Duration::from_millis(800));
        assert_eq!(VOICE_LATENCY_BUDGET.p95, Duration::from_millis(1500));
        assert_eq!(VOICE_LATENCY_BUDGET.barge_in, Duration::from_millis(150));
        assert_eq!(VOICE_LATENCY_BUDGET.eos_error_pct, 5);
    }

    #[test]
    fn voice_decision_matrix() {
        assert_eq!(
            voice_decision(VoiceActionClass::Read),
            VoiceApprovalDecision::AutoAllow
        );
        assert_eq!(
            voice_decision(VoiceActionClass::MutateReversible),
            VoiceApprovalDecision::PromptVoice
        );
        assert_eq!(
            voice_decision(VoiceActionClass::Destructive),
            VoiceApprovalDecision::RefuseVoiceMustUseGui
        );
    }

    #[test]
    fn notification_policy_round_trips_via_serde() {
        for p in [
            VoiceNotificationPolicy::Mute,
            VoiceNotificationPolicy::Summary,
            VoiceNotificationPolicy::Full,
            VoiceNotificationPolicy::InterruptOnly,
        ] {
            let s = serde_json::to_string(&p).unwrap();
            let back: VoiceNotificationPolicy = serde_json::from_str(&s).unwrap();
            assert_eq!(back, p);
        }
    }

    #[test]
    fn voice_subcap_default_cap_is_two_dollars() {
        assert!((VoiceSubcap::default().session_cap_usd - 2.0).abs() < f64::EPSILON);
    }

    #[test]
    fn voice_cost_thresholds_at_90_and_100() {
        let cap = VoiceSubcap::default();
        assert_eq!(classify_voice_cost(cap, 1.0), VoiceCostStatus::Ok);
        assert_eq!(classify_voice_cost(cap, 1.85), VoiceCostStatus::SoftWarn);
        assert_eq!(classify_voice_cost(cap, 2.5), VoiceCostStatus::HardStop);
    }

    #[test]
    fn negative_voice_cost_treated_as_ok() {
        assert_eq!(
            classify_voice_cost(VoiceSubcap::default(), -1.0),
            VoiceCostStatus::Ok
        );
    }

    #[test]
    fn voice_turn_audit_elapsed_subtracts_timestamps() {
        let audit = VoiceTurnAudit {
            session_id: "s".into(),
            turn_id: "t".into(),
            outcome: TurnOutcome::Interrupted,
            barge_in_at_ms: Some(120),
            stt_engine: SttEngine::ParakeetTdt,
            started_at_ms: 100,
            finished_at_ms: 800,
        };
        assert_eq!(audit.elapsed_ms(), 700);
    }

    #[test]
    fn voice_frame_serde_tags_kind() {
        let f = VoiceFrame::Eos { at_ms: 42 };
        let s = serde_json::to_string(&f).unwrap();
        assert!(s.contains("\"kind\":\"eos\""));
        let back: VoiceFrame = serde_json::from_str(&s).unwrap();
        assert_eq!(back, f);
    }

    #[test]
    fn voice_constants_are_stable() {
        assert_eq!(VOICE_WS_NAMESPACE, "session.voice");
        assert_eq!(VOICE_META_RPC, "v1.voice.meta");
    }
}
