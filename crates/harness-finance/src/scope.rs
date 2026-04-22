// IMPLEMENTS: D-369
//! Finance scope guard. Default is "analyst assistant" — research,
//! summarisation, formula derivation. Order placement and personalised
//! investment advice are refused outright.

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FinanceTask {
    Research,
    Summarise,
    DeriveFormula,
    Backtest,
    /// Refused.
    PlaceOrder,
    /// Refused.
    PersonalisedAdvice,
    /// Refused.
    DiscretionaryTrade,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum FinanceScopeError {
    #[error("task {0:?} is outside finance analyst scope — refused")]
    OutOfScope(FinanceTask),
}

pub fn ensure_in_scope(task: FinanceTask) -> Result<(), FinanceScopeError> {
    use FinanceTask::*;
    match task {
        Research | Summarise | DeriveFormula | Backtest => Ok(()),
        PlaceOrder | PersonalisedAdvice | DiscretionaryTrade => {
            Err(FinanceScopeError::OutOfScope(task))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn analyst_tasks_pass() {
        assert!(ensure_in_scope(FinanceTask::Research).is_ok());
        assert!(ensure_in_scope(FinanceTask::Backtest).is_ok());
    }

    #[test]
    fn order_placement_refused() {
        assert!(matches!(
            ensure_in_scope(FinanceTask::PlaceOrder),
            Err(FinanceScopeError::OutOfScope(_))
        ));
    }

    #[test]
    fn personalised_advice_refused() {
        assert!(matches!(
            ensure_in_scope(FinanceTask::PersonalisedAdvice),
            Err(FinanceScopeError::OutOfScope(_))
        ));
    }
}
