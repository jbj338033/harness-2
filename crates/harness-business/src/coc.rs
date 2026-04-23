// IMPLEMENTS: D-417
//! Contributor Covenant v2.1 enforcement workflow with the second-
//! mediator escalation step. Each report walks Correction →
//! Warning → Mediation → TemporaryBan → PermanentBan. Mediation is
//! an extra hop on top of the v2.1 default ladder so a single
//! moderator's call always has a second pair of eyes.

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CocStage {
    Reported,
    Correction,
    Warning,
    Mediation,
    TemporaryBan,
    PermanentBan,
    Closed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CocAction {
    Escalate,
    Resolve,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CocError {
    #[error("CoC stage {0:?} cannot escalate further")]
    AlreadyAtMax(CocStage),
}

pub fn advance_coc(stage: CocStage, action: CocAction) -> Result<CocStage, CocError> {
    use CocStage::*;
    match action {
        CocAction::Resolve => Ok(Closed),
        CocAction::Escalate => match stage {
            Reported => Ok(Correction),
            Correction => Ok(Warning),
            Warning => Ok(Mediation),
            Mediation => Ok(TemporaryBan),
            TemporaryBan => Ok(PermanentBan),
            PermanentBan | Closed => Err(CocError::AlreadyAtMax(stage)),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escalate_walks_through_mediation() {
        let stages = [
            CocStage::Reported,
            CocStage::Correction,
            CocStage::Warning,
            CocStage::Mediation,
            CocStage::TemporaryBan,
            CocStage::PermanentBan,
        ];
        let mut s = stages[0];
        for expected in &stages[1..] {
            s = advance_coc(s, CocAction::Escalate).unwrap();
            assert_eq!(s, *expected);
        }
    }

    #[test]
    fn resolve_jumps_to_closed() {
        assert_eq!(
            advance_coc(CocStage::Warning, CocAction::Resolve).unwrap(),
            CocStage::Closed
        );
    }

    #[test]
    fn escalate_past_permanent_ban_errors() {
        assert!(matches!(
            advance_coc(CocStage::PermanentBan, CocAction::Escalate),
            Err(CocError::AlreadyAtMax(_))
        ));
    }
}
