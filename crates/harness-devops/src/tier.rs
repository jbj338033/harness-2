// IMPLEMENTS: D-254
//! `ToolTier` — kubectl, terraform, and cloud CLI tools tag every
//! sub-command at one of these tiers. The capabilities layer enforces
//! per-tier approval gating.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolTier {
    Read,
    MutateReversible,
    Destructive,
}

impl ToolTier {
    #[must_use]
    pub fn requires_approval(self) -> bool {
        !matches!(self, ToolTier::Read)
    }

    #[must_use]
    pub fn dual_signoff_required(self) -> bool {
        matches!(self, ToolTier::Destructive)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_skips_approval() {
        assert!(!ToolTier::Read.requires_approval());
        assert!(!ToolTier::Read.dual_signoff_required());
    }

    #[test]
    fn destructive_needs_dual_signoff() {
        assert!(ToolTier::Destructive.requires_approval());
        assert!(ToolTier::Destructive.dual_signoff_required());
    }

    #[test]
    fn mutate_reversible_single_approval() {
        assert!(ToolTier::MutateReversible.requires_approval());
        assert!(!ToolTier::MutateReversible.dual_signoff_required());
    }
}
