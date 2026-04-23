// IMPLEMENTS: D-411
//! ASC 606 / IFRS 15 monthly contract-liability amortisation. Cash
//! lands at sale; revenue is recognised straight-line across the
//! service period. The carryover stays as contract liability until
//! recognised.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ContractLiabilityRow {
    pub contract_id: u64,
    pub total_cents: u64,
    pub months: u32,
}

/// Monthly recognition amount in cents. Last-month residual absorbs
/// rounding so the sum exactly matches `total_cents`.
#[must_use]
pub fn monthly_recognition(row: ContractLiabilityRow, month_index: u32) -> u64 {
    if row.months == 0 || month_index >= row.months {
        return 0;
    }
    let base = row.total_cents / u64::from(row.months);
    if month_index + 1 == row.months {
        row.total_cents - base * u64::from(row.months - 1)
    } else {
        base
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row() -> ContractLiabilityRow {
        ContractLiabilityRow {
            contract_id: 1,
            total_cents: 12_000,
            months: 12,
        }
    }

    #[test]
    fn even_split_amortises_evenly() {
        for m in 0..12 {
            assert_eq!(monthly_recognition(row(), m), 1_000);
        }
    }

    #[test]
    fn rounding_residual_lands_in_last_month() {
        let r = ContractLiabilityRow {
            contract_id: 2,
            total_cents: 100,
            months: 3,
        };
        assert_eq!(monthly_recognition(r, 0), 33);
        assert_eq!(monthly_recognition(r, 1), 33);
        assert_eq!(monthly_recognition(r, 2), 34);
    }

    #[test]
    fn out_of_range_month_returns_zero() {
        assert_eq!(monthly_recognition(row(), 12), 0);
    }

    #[test]
    fn zero_month_contract_returns_zero() {
        let r = ContractLiabilityRow {
            contract_id: 3,
            total_cents: 100,
            months: 0,
        };
        assert_eq!(monthly_recognition(r, 0), 0);
    }
}
