// IMPLEMENTS: D-414
//! Triple-gate incorporation trigger. Form a Korean 1인 주식회사
//! when ANY of: first paid contract closes, monthly active user
//! count reaches 1k, monthly sponsorship reaches $3k. DE C-corp is
//! pushed off until ARR ≥ $300k.

use serde::{Deserialize, Serialize};

pub const MAU_GATE: u32 = 1_000;
pub const MONTHLY_SPONSOR_USD_GATE: u32 = 3_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct IncorporationGate {
    pub first_paid_contract_signed: bool,
    pub monthly_active_users: u32,
    pub monthly_sponsorship_usd: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IncorporationVerdict {
    Hold,
    FormKoreanCorp,
}

#[must_use]
pub fn evaluate_incorporation(g: IncorporationGate) -> IncorporationVerdict {
    if g.first_paid_contract_signed
        || g.monthly_active_users >= MAU_GATE
        || g.monthly_sponsorship_usd >= MONTHLY_SPONSOR_USD_GATE
    {
        IncorporationVerdict::FormKoreanCorp
    } else {
        IncorporationVerdict::Hold
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn baseline() -> IncorporationGate {
        IncorporationGate {
            first_paid_contract_signed: false,
            monthly_active_users: 0,
            monthly_sponsorship_usd: 0,
        }
    }

    #[test]
    fn baseline_holds() {
        assert_eq!(
            evaluate_incorporation(baseline()),
            IncorporationVerdict::Hold
        );
    }

    #[test]
    fn first_paid_contract_triggers() {
        let mut g = baseline();
        g.first_paid_contract_signed = true;
        assert_eq!(
            evaluate_incorporation(g),
            IncorporationVerdict::FormKoreanCorp
        );
    }

    #[test]
    fn mau_gate_triggers_at_threshold() {
        let mut g = baseline();
        g.monthly_active_users = MAU_GATE;
        assert_eq!(
            evaluate_incorporation(g),
            IncorporationVerdict::FormKoreanCorp
        );
    }

    #[test]
    fn sponsorship_gate_triggers_at_threshold() {
        let mut g = baseline();
        g.monthly_sponsorship_usd = MONTHLY_SPONSOR_USD_GATE;
        assert_eq!(
            evaluate_incorporation(g),
            IncorporationVerdict::FormKoreanCorp
        );
    }
}
