// IMPLEMENTS: D-274
//! `session.undoLastTurn()` request envelope. The actual revert
//! engine reuses the D-091 ExternalEdit trash for filesystem
//! mutations and dispatches a per-tool *compensate Act* for tool
//! invocations. Some side effects are not undoable (sent email, paid
//! invoice); we surface those explicitly via `UndoLastTurnError`.

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UndoLastTurnRequest {
    pub session_id: String,
    pub turn_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UndoEffectKind {
    /// File mutation — D-091 trash restore.
    FileMutation,
    /// Tool call with a compensating Act registered.
    ReversibleTool,
    /// Tool call with no compensation path (eg. send-mail, pay).
    IrreversibleTool,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum UndoLastTurnError {
    #[error("turn {0} included irreversible side effect: {1:?}")]
    Irreversible(String, UndoEffectKind),
    #[error("turn {0} not found")]
    UnknownTurn(String),
}

pub fn evaluate_undo(
    req: &UndoLastTurnRequest,
    effects: &[UndoEffectKind],
    turn_known: bool,
) -> Result<(), UndoLastTurnError> {
    if !turn_known {
        return Err(UndoLastTurnError::UnknownTurn(req.turn_id.clone()));
    }
    if let Some(kind) = effects
        .iter()
        .find(|k| matches!(k, UndoEffectKind::IrreversibleTool))
    {
        return Err(UndoLastTurnError::Irreversible(req.turn_id.clone(), *kind));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req() -> UndoLastTurnRequest {
        UndoLastTurnRequest {
            session_id: "s".into(),
            turn_id: "t".into(),
        }
    }

    #[test]
    fn reversible_effects_undo_ok() {
        assert!(
            evaluate_undo(
                &req(),
                &[UndoEffectKind::FileMutation, UndoEffectKind::ReversibleTool],
                true
            )
            .is_ok()
        );
    }

    #[test]
    fn irreversible_blocks_undo() {
        let r = evaluate_undo(&req(), &[UndoEffectKind::IrreversibleTool], true);
        assert!(matches!(r, Err(UndoLastTurnError::Irreversible(_, _))));
    }

    #[test]
    fn unknown_turn_returns_error() {
        let r = evaluate_undo(&req(), &[], false);
        assert!(matches!(r, Err(UndoLastTurnError::UnknownTurn(_))));
    }
}
