// IMPLEMENTS: D-270
//! Semantic RPC aliases. The non-developer vocabulary fronts the
//! real RPC namespace so a Web/Mobile user clicks "Undo last step"
//! and the surface dispatches `session.undo_last_turn`.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SemanticAlias {
    pub friendly: &'static str,
    pub rpc_method: &'static str,
}

const ALIASES: &[SemanticAlias] = &[
    SemanticAlias {
        friendly: "session.undoLastTurn",
        rpc_method: "v1.session.undo_last_turn",
    },
    SemanticAlias {
        friendly: "memory.export",
        rpc_method: "v1.memory.export",
    },
    SemanticAlias {
        friendly: "agent.askForHelp",
        rpc_method: "v1.agent.ask_for_help",
    },
    SemanticAlias {
        friendly: "session.startOver",
        rpc_method: "v1.session.start_over",
    },
    SemanticAlias {
        friendly: "settings.changeModel",
        rpc_method: "v1.settings.change_model",
    },
];

#[must_use]
pub fn all_aliases() -> &'static [SemanticAlias] {
    ALIASES
}

#[must_use]
pub fn resolve_alias(friendly: &str) -> Option<&'static str> {
    ALIASES
        .iter()
        .find(|a| a.friendly == friendly)
        .map(|a| a.rpc_method)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn undo_alias_resolves_to_namespaced_rpc() {
        assert_eq!(
            resolve_alias("session.undoLastTurn"),
            Some("v1.session.undo_last_turn")
        );
    }

    #[test]
    fn unknown_alias_returns_none() {
        assert!(resolve_alias("session.summonGenie").is_none());
    }

    #[test]
    fn five_aliases_registered() {
        assert_eq!(all_aliases().len(), 5);
    }
}
