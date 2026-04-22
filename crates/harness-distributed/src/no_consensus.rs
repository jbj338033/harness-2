// IMPLEMENTS: D-446
//! Explicit refusal to ship Raft / Paxos / Zab. Single logical writer
//! per session is the invariant; CAP=AP, PACELC=PA/EL.

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NoConsensusContract {
    Raft,
    Paxos,
    Zab,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ConsensusRefusalError {
    #[error("{0:?} is explicitly out of scope — single writer per session, CAP=AP")]
    Refused(NoConsensusContract),
}

pub fn refuse_consensus_protocol(p: NoConsensusContract) -> Result<(), ConsensusRefusalError> {
    Err(ConsensusRefusalError::Refused(p))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raft_refused() {
        assert_eq!(
            refuse_consensus_protocol(NoConsensusContract::Raft),
            Err(ConsensusRefusalError::Refused(NoConsensusContract::Raft))
        );
    }

    #[test]
    fn paxos_refused() {
        assert!(refuse_consensus_protocol(NoConsensusContract::Paxos).is_err());
    }

    #[test]
    fn zab_refused() {
        assert!(refuse_consensus_protocol(NoConsensusContract::Zab).is_err());
    }
}
