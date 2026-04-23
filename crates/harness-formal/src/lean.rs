// IMPLEMENTS: D-325
//! AI-Assisted Lean 4 invariant proof descriptors. Each
//! `LeanProofObligation` names an invariant that exists in Rust and
//! must hold in the Lean translation. Status is one of `Pending`,
//! `Discharged`, or `OpenSubgoal(name)` so a stalled obligation can
//! be picked up by a different tactic provider.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RustToLeanToolchain {
    Aeneas,
    Thrust,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TacticProvider {
    DeepSeekProverV2,
    Leanstral,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LeanProofStatus {
    Pending,
    Discharged,
    OpenSubgoal { name: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeanProofObligation {
    pub invariant_name: String,
    pub source_module: String,
    pub toolchain: RustToLeanToolchain,
    pub tactic: TacticProvider,
    pub status: LeanProofStatus,
}

impl LeanProofObligation {
    #[must_use]
    pub fn is_blocking(&self) -> bool {
        !matches!(self.status, LeanProofStatus::Discharged)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn obligation(status: LeanProofStatus) -> LeanProofObligation {
        LeanProofObligation {
            invariant_name: "events_append_only".into(),
            source_module: "harness-storage::events".into(),
            toolchain: RustToLeanToolchain::Aeneas,
            tactic: TacticProvider::DeepSeekProverV2,
            status,
        }
    }

    #[test]
    fn discharged_does_not_block() {
        assert!(!obligation(LeanProofStatus::Discharged).is_blocking());
    }

    #[test]
    fn pending_blocks() {
        assert!(obligation(LeanProofStatus::Pending).is_blocking());
    }

    #[test]
    fn open_subgoal_blocks_with_name() {
        let o = obligation(LeanProofStatus::OpenSubgoal {
            name: "monotone_seq".into(),
        });
        assert!(o.is_blocking());
        match o.status {
            LeanProofStatus::OpenSubgoal { name } => assert_eq!(name, "monotone_seq"),
            _ => panic!("expected open subgoal"),
        }
    }

    #[test]
    fn obligation_round_trips() {
        let o = obligation(LeanProofStatus::Pending);
        let s = serde_json::to_string(&o).unwrap();
        let back: LeanProofObligation = serde_json::from_str(&s).unwrap();
        assert_eq!(back, o);
    }
}
