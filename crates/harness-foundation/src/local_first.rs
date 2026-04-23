// IMPLEMENTS: D-426
//! Local Inference Fallback profile. Default is Qwen3-Coder-Next
//! 80B-A3B on dual RTX 4090 — the cheapest known config that holds
//! up against the frontier on coding benchmarks.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalInferenceProfile {
    pub model_id: String,
    pub gpus: u8,
    pub vram_per_gpu_gib: u16,
    pub quantisation: String,
}

#[must_use]
pub fn default_local_profile() -> LocalInferenceProfile {
    LocalInferenceProfile {
        model_id: "qwen3-coder-next-80b-a3b".into(),
        gpus: 2,
        vram_per_gpu_gib: 24,
        quantisation: "q4_k_m".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_profile_targets_dual_4090() {
        let p = default_local_profile();
        assert_eq!(p.gpus, 2);
        assert_eq!(p.vram_per_gpu_gib, 24);
        assert!(p.model_id.contains("80b"));
    }

    #[test]
    fn profile_round_trips() {
        let p = default_local_profile();
        let s = serde_json::to_string(&p).unwrap();
        let back: LocalInferenceProfile = serde_json::from_str(&s).unwrap();
        assert_eq!(back, p);
    }
}
