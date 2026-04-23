// IMPLEMENTS: D-428
//! Provider collapse trigger. When a provider has been unreachable
//! for >= 30 minutes, we surface a recommendation to export the
//! Agent Trace so the user has a portable record before things get
//! worse.

use serde::{Deserialize, Serialize};

pub const PROVIDER_COLLAPSE_TRIGGER_MINUTES: u32 = 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderCollapseRecommendation {
    Hold,
    SuggestAgentTraceExport,
}

#[must_use]
pub fn evaluate_collapse(unreachable_minutes: u32) -> ProviderCollapseRecommendation {
    if unreachable_minutes >= PROVIDER_COLLAPSE_TRIGGER_MINUTES {
        ProviderCollapseRecommendation::SuggestAgentTraceExport
    } else {
        ProviderCollapseRecommendation::Hold
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn under_thirty_holds() {
        assert_eq!(evaluate_collapse(15), ProviderCollapseRecommendation::Hold);
    }

    #[test]
    fn at_thirty_suggests_export() {
        assert_eq!(
            evaluate_collapse(PROVIDER_COLLAPSE_TRIGGER_MINUTES),
            ProviderCollapseRecommendation::SuggestAgentTraceExport
        );
    }
}
