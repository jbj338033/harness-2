// IMPLEMENTS: D-285
//! STT default. Claude has no audio input; we route every audio
//! attachment through Whisper large-v3-turbo locally so the daemon stays
//! provider-agnostic. The struct is config-only — the actual ffmpeg /
//! whisper invocation lives in the binary that consumes this crate.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SttBackend {
    #[default]
    WhisperLargeV3Turbo,
    /// Future: provider-hosted alternative.
    OpenAiWhisperApi,
    /// Skip transcription entirely (audio still gets a caption slot).
    Disabled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SttConfig {
    pub backend: SttBackend,
    pub model_path: Option<String>,
    pub language: Option<String>,
    pub min_silence_ms: u32,
}

impl Default for SttConfig {
    fn default() -> Self {
        Self {
            backend: SttBackend::WhisperLargeV3Turbo,
            model_path: None,
            language: None,
            min_silence_ms: 500,
        }
    }
}

impl SttConfig {
    /// Whether this config will produce any text. `Disabled` returns
    /// false; any local or hosted backend returns true.
    #[must_use]
    pub fn is_active(&self) -> bool {
        !matches!(self.backend, SttBackend::Disabled)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_backend_is_whisper_large_v3_turbo() {
        let c = SttConfig::default();
        assert_eq!(c.backend, SttBackend::WhisperLargeV3Turbo);
        assert!(c.is_active());
    }

    #[test]
    fn disabled_backend_marks_inactive() {
        let c = SttConfig {
            backend: SttBackend::Disabled,
            ..SttConfig::default()
        };
        assert!(!c.is_active());
    }

    #[test]
    fn config_serde_round_trip() {
        let c = SttConfig {
            backend: SttBackend::WhisperLargeV3Turbo,
            model_path: Some("/opt/whisper/large-v3-turbo.bin".into()),
            language: Some("ko".into()),
            min_silence_ms: 750,
        };
        let s = serde_json::to_string(&c).unwrap();
        let back: SttConfig = serde_json::from_str(&s).unwrap();
        assert_eq!(back, c);
    }
}
