// IMPLEMENTS: D-390
//! Physical tool tier — strictly above `Destructive`. Reversibility
//! collapses to zero on the life-safety axis (a crushed finger isn't
//! undoable), so the planner must never auto-promote a Physical
//! action regardless of approval streak (D-191 habituation defence).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PhysicalTier {
    /// Read-only sensor query (LIDAR ping, joint position read).
    Read,
    /// Reversible motion (move within envelope, cancel-able).
    Reversible,
    /// Mutating but rollback-feasible (gripper open/close on inert
    /// object).
    Destructive,
    /// Physical interaction with people or fragile world. Capped here.
    Physical,
}

#[must_use]
pub fn dominates_destructive(t: PhysicalTier) -> bool {
    matches!(t, PhysicalTier::Physical)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn physical_dominates_destructive() {
        assert!(PhysicalTier::Physical > PhysicalTier::Destructive);
        assert!(dominates_destructive(PhysicalTier::Physical));
    }

    #[test]
    fn read_is_lowest() {
        assert!(PhysicalTier::Read < PhysicalTier::Reversible);
        assert!(!dominates_destructive(PhysicalTier::Reversible));
    }
}
