// IMPLEMENTS: D-412
//! Commercial contract template registry. Three documents that
//! every paid customer needs: MSA (Master Services Agreement), DPA
//! (Data Processing Addendum), Enterprise SOW (Statement of Work).
//! Bodies live in the `docs/` directory; this module just registers
//! the slugs.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContractTemplate {
    MasterServicesAgreement,
    DataProcessingAddendum,
    EnterpriseSow,
}

#[must_use]
pub fn registered_templates() -> &'static [ContractTemplate] {
    use ContractTemplate::*;
    const ALL: &[ContractTemplate] = &[
        MasterServicesAgreement,
        DataProcessingAddendum,
        EnterpriseSow,
    ];
    ALL
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn three_templates_registered() {
        assert_eq!(registered_templates().len(), 3);
    }

    #[test]
    fn dpa_present() {
        assert!(registered_templates().contains(&ContractTemplate::DataProcessingAddendum));
    }
}
