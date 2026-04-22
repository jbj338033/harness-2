// IMPLEMENTS: D-370
//! `FinancialTier` is intentionally a separate enum from R40's
//! `PaymentTier`. Mixing the two at a call site would let "the user
//! has paid" creep into "the user is allowed to trade" — the Rust
//! type system makes that mistake unrepresentable here.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FinancialTier {
    /// Public information only — research mode that any anonymous
    /// session can run.
    Public,
    /// User has signed the in-product analyst agreement.
    Analyst,
    /// Institutional desk — additional auditing requirements kick in
    /// (D-374 enabled formats).
    Institutional,
}

impl FinancialTier {
    #[must_use]
    pub fn is_at_least(self, other: Self) -> bool {
        self >= other
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordering_is_public_then_analyst_then_institutional() {
        assert!(FinancialTier::Public < FinancialTier::Analyst);
        assert!(FinancialTier::Analyst < FinancialTier::Institutional);
    }

    #[test]
    fn institutional_dominates() {
        assert!(FinancialTier::Institutional.is_at_least(FinancialTier::Public));
        assert!(!FinancialTier::Public.is_at_least(FinancialTier::Analyst));
    }
}
