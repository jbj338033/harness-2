// IMPLEMENTS: D-255
//! Pointer to the SRE tool crates. Each entry names the crate that
//! contains the corresponding `harness_tools::Tool` impl and the
//! `ToolTier` we expect every sub-command to default to.

use crate::tier::ToolTier;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SreToolCrate {
    pub crate_name: &'static str,
    pub default_tier: ToolTier,
    pub touches_cluster_state: bool,
}

#[must_use]
pub fn registered_sre_crates() -> &'static [SreToolCrate] {
    const ROW: &[SreToolCrate] = &[
        SreToolCrate {
            crate_name: "harness-tools-kubectl",
            default_tier: ToolTier::MutateReversible,
            touches_cluster_state: true,
        },
        SreToolCrate {
            crate_name: "harness-tools-terraform",
            default_tier: ToolTier::Destructive,
            touches_cluster_state: true,
        },
        SreToolCrate {
            crate_name: "harness-tools-cloudcli",
            default_tier: ToolTier::Destructive,
            touches_cluster_state: true,
        },
    ];
    ROW
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn three_crates_registered() {
        assert_eq!(registered_sre_crates().len(), 3);
    }

    #[test]
    fn terraform_default_is_destructive() {
        let row = registered_sre_crates()
            .iter()
            .find(|c| c.crate_name == "harness-tools-terraform")
            .unwrap();
        assert_eq!(row.default_tier, ToolTier::Destructive);
    }
}
