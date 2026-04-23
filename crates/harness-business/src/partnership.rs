// IMPLEMENTS: D-416
//! Partnership milestone ladder. AWS Activate $1k credit → first
//! ship → accelerator entry. We pin the order so a later milestone
//! can't be claimed without the earlier one being attested.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PartnershipMilestone {
    AwsActivate1k,
    FirstShip,
    AcceleratorEntry,
}

#[must_use]
pub fn all_partnership_milestones() -> &'static [PartnershipMilestone] {
    use PartnershipMilestone::*;
    const ALL: &[PartnershipMilestone] = &[AwsActivate1k, FirstShip, AcceleratorEntry];
    ALL
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn three_milestones_in_order() {
        assert_eq!(all_partnership_milestones().len(), 3);
        assert!(PartnershipMilestone::AwsActivate1k < PartnershipMilestone::FirstShip);
        assert!(PartnershipMilestone::FirstShip < PartnershipMilestone::AcceleratorEntry);
    }
}
