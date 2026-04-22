// IMPLEMENTS: D-404, D-405, D-406, D-407, D-408, D-409
//! XR surface — sixth `SurfaceKind`, spatial intent RPC, 30-minute
//! health guard with `HealthHold`, dual confirm (pinch + voice, gaze
//! alone refused), and gaze redaction so raw gaze never crosses the
//! provider boundary (GDPR Art 9 — gaze is biometric).

use serde::{Deserialize, Serialize};
use std::time::Duration;

// ----- D-404: SurfaceKind::Xr is the sixth surface -----

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SurfaceKind {
    Cli,
    Tui,
    Web,
    Desktop,
    Voice,
    Xr,
}

// ----- D-405: InputSource expansion (additive only — no new event kinds) -----

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputSource {
    Keyboard,
    Mouse,
    Touch,
    Voice,
    /// Pinch (mid-air) — XR primary commit gesture.
    Pinch,
    /// Gaze direction sample — never the sole authority for an action
    /// (D-408). Raw gaze stays on-device (D-409).
    Gaze,
    /// Hand pose / generic spatial gesture.
    SpatialGesture,
}

impl InputSource {
    /// Single-source gaze cannot commit. The dual-confirm rule lives in
    /// [`commit_decision`].
    #[must_use]
    pub fn is_xr(self) -> bool {
        matches!(
            self,
            InputSource::Pinch | InputSource::Gaze | InputSource::SpatialGesture
        )
    }
}

// ----- D-406: session.spatialIntent RPC + payload -----

pub const SPATIAL_INTENT_RPC: &str = "v1.session.spatial_intent";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpatialIntent {
    SelectTarget,
    DismissPanel,
    SummonAgent,
    BeginDictation,
    EndDictation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpatialIntentEnvelope {
    pub session_id: String,
    pub intent: SpatialIntent,
    /// Sources that contributed (eg. `[Pinch, Voice]`). Pose / gaze
    /// vectors stay on-device — only the resolved intent crosses the
    /// daemon boundary.
    pub sources: Vec<InputSource>,
    pub at_ms: i64,
}

// ----- D-407: 30-minute health guard + HealthHold -----

pub const XR_HEALTH_HARD_CAP: Duration = Duration::from_secs(30 * 60);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum XrHealthState {
    Active,
    /// Soft warning at 25 minutes — surface a banner but session keeps
    /// running.
    Warn,
    /// Hard cap — daemon refuses new turns until session is reset.
    HealthHold,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgeBand {
    Adult,
    /// Per D-407, minors are blocked from XR sessions outright (SaMD
    /// boundary reaffirmed).
    Minor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum XrAdmissionDecision {
    Admit,
    RefuseMinor,
    RefuseHealthHold,
}

#[must_use]
pub fn classify_health(elapsed: Duration) -> XrHealthState {
    if elapsed >= XR_HEALTH_HARD_CAP {
        XrHealthState::HealthHold
    } else if elapsed >= Duration::from_secs(25 * 60) {
        XrHealthState::Warn
    } else {
        XrHealthState::Active
    }
}

#[must_use]
pub fn admit_xr(age: AgeBand, elapsed: Duration) -> XrAdmissionDecision {
    if matches!(age, AgeBand::Minor) {
        return XrAdmissionDecision::RefuseMinor;
    }
    if matches!(classify_health(elapsed), XrHealthState::HealthHold) {
        return XrAdmissionDecision::RefuseHealthHold;
    }
    XrAdmissionDecision::Admit
}

// ----- D-408: pinch + voice dual confirm (gaze alone refused) -----

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommitDecision {
    Allow,
    /// Gaze-only or otherwise insufficient — refuse the commit.
    RefuseGazeOnly,
    /// Missing one of the two required factors (we have pinch but no
    /// voice, or vice versa).
    RefuseMissingFactor,
}

#[must_use]
pub fn commit_decision(sources: &[InputSource]) -> CommitDecision {
    let has_pinch = sources.contains(&InputSource::Pinch);
    let has_voice = sources.contains(&InputSource::Voice);
    let only_gaze = sources.iter().all(|s| matches!(s, InputSource::Gaze));
    if only_gaze && !sources.is_empty() {
        return CommitDecision::RefuseGazeOnly;
    }
    if has_pinch && has_voice {
        return CommitDecision::Allow;
    }
    CommitDecision::RefuseMissingFactor
}

// ----- D-409: gaze redaction + whoami row -----

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GazeSample {
    pub pitch_rad: f32,
    pub yaw_rad: f32,
    pub at_ms: i64,
}

/// Provider-bound payload. Raw gaze samples MUST be stripped — only the
/// resolved intent stays. Returns the redacted JSON the provider sees.
#[must_use]
pub fn redact_for_provider(envelope: &SpatialIntentEnvelope) -> serde_json::Value {
    let kept_sources: Vec<InputSource> = envelope
        .sources
        .iter()
        .copied()
        .filter(|s| !matches!(s, InputSource::Gaze))
        .collect();
    serde_json::json!({
        "session_id": envelope.session_id,
        "intent": envelope.intent,
        "sources": kept_sources,
        "at_ms": envelope.at_ms,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct XrWhoAmIRow {
    pub surface: SurfaceKind,
    pub age_band: AgeBand,
    pub gaze_redaction: bool,
    pub health_state: XrHealthState,
}

impl XrWhoAmIRow {
    #[must_use]
    pub fn new(age_band: AgeBand, health_state: XrHealthState) -> Self {
        Self {
            surface: SurfaceKind::Xr,
            age_band,
            gaze_redaction: true,
            health_state,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xr_is_sixth_surface_kind() {
        let all = [
            SurfaceKind::Cli,
            SurfaceKind::Tui,
            SurfaceKind::Web,
            SurfaceKind::Desktop,
            SurfaceKind::Voice,
            SurfaceKind::Xr,
        ];
        assert_eq!(all.len(), 6);
        assert!(all.contains(&SurfaceKind::Xr));
    }

    #[test]
    fn input_source_xr_subset() {
        assert!(InputSource::Pinch.is_xr());
        assert!(InputSource::Gaze.is_xr());
        assert!(InputSource::SpatialGesture.is_xr());
        assert!(!InputSource::Voice.is_xr());
        assert!(!InputSource::Keyboard.is_xr());
    }

    #[test]
    fn input_source_serde_is_additive_snake_case() {
        let s = serde_json::to_string(&InputSource::SpatialGesture).unwrap();
        assert_eq!(s, "\"spatial_gesture\"");
    }

    #[test]
    fn spatial_intent_rpc_is_namespaced() {
        assert_eq!(SPATIAL_INTENT_RPC, "v1.session.spatial_intent");
    }

    #[test]
    fn classify_health_thresholds_at_25_and_30_min() {
        assert_eq!(
            classify_health(Duration::from_secs(10 * 60)),
            XrHealthState::Active
        );
        assert_eq!(
            classify_health(Duration::from_secs(26 * 60)),
            XrHealthState::Warn
        );
        assert_eq!(
            classify_health(Duration::from_secs(31 * 60)),
            XrHealthState::HealthHold
        );
    }

    #[test]
    fn minor_is_refused_regardless_of_health() {
        assert_eq!(
            admit_xr(AgeBand::Minor, Duration::from_secs(0)),
            XrAdmissionDecision::RefuseMinor
        );
    }

    #[test]
    fn adult_at_hard_cap_hits_health_hold() {
        assert_eq!(
            admit_xr(AgeBand::Adult, XR_HEALTH_HARD_CAP),
            XrAdmissionDecision::RefuseHealthHold
        );
    }

    #[test]
    fn adult_under_cap_admitted() {
        assert_eq!(
            admit_xr(AgeBand::Adult, Duration::from_secs(60)),
            XrAdmissionDecision::Admit
        );
    }

    #[test]
    fn pinch_and_voice_commits() {
        assert_eq!(
            commit_decision(&[InputSource::Pinch, InputSource::Voice]),
            CommitDecision::Allow
        );
    }

    #[test]
    fn gaze_only_is_refused() {
        assert_eq!(
            commit_decision(&[InputSource::Gaze]),
            CommitDecision::RefuseGazeOnly
        );
    }

    #[test]
    fn pinch_alone_is_missing_factor() {
        assert_eq!(
            commit_decision(&[InputSource::Pinch]),
            CommitDecision::RefuseMissingFactor
        );
    }

    #[test]
    fn redaction_strips_gaze_sources() {
        let env = SpatialIntentEnvelope {
            session_id: "s".into(),
            intent: SpatialIntent::SelectTarget,
            sources: vec![InputSource::Pinch, InputSource::Gaze, InputSource::Voice],
            at_ms: 1,
        };
        let v = redact_for_provider(&env);
        let arr = v["sources"].as_array().unwrap();
        assert_eq!(arr.len(), 2);
        let s = serde_json::to_string(&v).unwrap();
        assert!(!s.contains("gaze"));
    }

    #[test]
    fn whoami_row_advertises_redaction_on_by_default() {
        let row = XrWhoAmIRow::new(AgeBand::Adult, XrHealthState::Active);
        assert_eq!(row.surface, SurfaceKind::Xr);
        assert!(row.gaze_redaction);
    }
}
