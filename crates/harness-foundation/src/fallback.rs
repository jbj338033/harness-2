// IMPLEMENTS: D-425
//! Provider Fallback ladder. Five tiers, attempted in order. Anthropic
//! concentration (~40%) is the motivating risk — if that family is
//! down or rate-limited, we walk down to the local model rather than
//! hard-failing.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderFallbackTier {
    AnthropicPrimary,
    AnthropicSecondary,
    OpenaiPrimary,
    GooglePrimary,
    LocalInference,
}

#[must_use]
pub fn all_fallback_tiers() -> &'static [ProviderFallbackTier] {
    use ProviderFallbackTier::*;
    const ALL: &[ProviderFallbackTier] = &[
        AnthropicPrimary,
        AnthropicSecondary,
        OpenaiPrimary,
        GooglePrimary,
        LocalInference,
    ];
    ALL
}

/// Walks one step further down the ladder. `None` means we've
/// exhausted every tier and should surface a hard failure.
#[must_use]
pub fn next_tier(current: ProviderFallbackTier) -> Option<ProviderFallbackTier> {
    let tiers = all_fallback_tiers();
    let idx = tiers.iter().position(|t| *t == current)?;
    tiers.get(idx + 1).copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn five_tiers() {
        assert_eq!(all_fallback_tiers().len(), 5);
    }

    #[test]
    fn primary_falls_through_to_local() {
        let mut t = ProviderFallbackTier::AnthropicPrimary;
        let mut steps = 0;
        while let Some(next) = next_tier(t) {
            t = next;
            steps += 1;
        }
        assert_eq!(t, ProviderFallbackTier::LocalInference);
        assert_eq!(steps, 4);
    }

    #[test]
    fn local_has_no_next() {
        assert!(next_tier(ProviderFallbackTier::LocalInference).is_none());
    }
}
