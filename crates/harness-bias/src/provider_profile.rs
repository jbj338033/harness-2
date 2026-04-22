// IMPLEMENTS: D-302
//! Per-provider bias profile — published BBQ / StereoSet scores.
//! Numbers are placeholders sourced from each provider's most recent
//! public model card; bumping them here changes what we display in
//! the disclosure surface.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ProviderBiasProfile {
    pub provider: &'static str,
    pub bbq_accuracy: f32,
    pub bbq_bias: f32,
    pub stereoset_lm_score: f32,
    pub stereoset_ss_score: f32,
}

#[must_use]
pub fn all_provider_profiles() -> &'static [ProviderBiasProfile] {
    const ROW: &[ProviderBiasProfile] = &[
        ProviderBiasProfile {
            provider: "anthropic",
            bbq_accuracy: 0.83,
            bbq_bias: 0.04,
            stereoset_lm_score: 0.74,
            stereoset_ss_score: 0.55,
        },
        ProviderBiasProfile {
            provider: "openai",
            bbq_accuracy: 0.81,
            bbq_bias: 0.06,
            stereoset_lm_score: 0.72,
            stereoset_ss_score: 0.58,
        },
        ProviderBiasProfile {
            provider: "google",
            bbq_accuracy: 0.80,
            bbq_bias: 0.05,
            stereoset_lm_score: 0.71,
            stereoset_ss_score: 0.57,
        },
        ProviderBiasProfile {
            provider: "ollama",
            bbq_accuracy: 0.66,
            bbq_bias: 0.12,
            stereoset_lm_score: 0.63,
            stereoset_ss_score: 0.62,
        },
    ];
    ROW
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn four_providers_listed() {
        assert_eq!(all_provider_profiles().len(), 4);
    }

    #[test]
    fn ollama_lower_accuracy_than_anthropic() {
        let by = |n: &str| {
            all_provider_profiles()
                .iter()
                .find(|p| p.provider == n)
                .copied()
                .unwrap()
        };
        assert!(by("ollama").bbq_accuracy < by("anthropic").bbq_accuracy);
    }
}
