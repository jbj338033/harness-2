// IMPLEMENTS: D-338, D-339, D-341, D-342, D-343
//! Autonomy level matrix.
//!
//! - **L0** — read-only browse, no Act emission.
//! - **L1** — Act allowed but every call is a hard interactive prompt.
//! - **L2** — recurring patterns may be granted for the session.
//! - **L3** — wide grants permitted, but EU AI Act Art 14 oversight is
//!   still mandatory. This is the highest level the daemon will run a
//!   "high-risk" model on.
//! - **L4** — full grants, prompt only on novel destructive actions.
//! - **L5** — fully autonomous loop. Refused for any model whose
//!   capability card declares ASL ≥ 3.

use harness_auth::{PrivateKey, PublicKey, SignatureBytes};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AutonomyLevel {
    L0,
    L1,
    L2,
    L3,
    L4,
    L5,
}

impl AutonomyLevel {
    #[must_use]
    pub fn as_u8(self) -> u8 {
        match self {
            Self::L0 => 0,
            Self::L1 => 1,
            Self::L2 => 2,
            Self::L3 => 3,
            Self::L4 => 4,
            Self::L5 => 5,
        }
    }

    pub const ALL: &'static [AutonomyLevel] =
        &[Self::L0, Self::L1, Self::L2, Self::L3, Self::L4, Self::L5];

    /// EU AI Act Art 14 cap — high-risk models may not run unattended
    /// above this level (D-338).
    pub const EU_HIGH_RISK_MAX: AutonomyLevel = Self::L3;
}

// ----- D-339: approval auto-map -----

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalPolicy {
    /// Every Act prompts the user.
    PromptEveryAct,
    /// Recurring patterns may be granted for the session; novel ones
    /// still prompt.
    PromptOnNovel,
    /// Pre-existing grants run silently; new destructive actions still
    /// prompt.
    PromptOnDestructive,
    /// Fully autonomous — no prompts.
    SilentAllow,
    /// L0 mode — Act emission rejected outright.
    NoAct,
}

#[must_use]
pub fn policy_for(level: AutonomyLevel) -> ApprovalPolicy {
    match level {
        AutonomyLevel::L0 => ApprovalPolicy::NoAct,
        AutonomyLevel::L1 => ApprovalPolicy::PromptEveryAct,
        AutonomyLevel::L2 | AutonomyLevel::L3 => ApprovalPolicy::PromptOnNovel,
        AutonomyLevel::L4 => ApprovalPolicy::PromptOnDestructive,
        AutonomyLevel::L5 => ApprovalPolicy::SilentAllow,
    }
}

// ----- D-341: capability card + ASL lock -----

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AslTier {
    Asl1,
    Asl2,
    Asl3,
    Asl4,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityCard {
    pub provider: String,
    pub model: String,
    pub asl: AslTier,
    pub eu_high_risk: bool,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum AutonomyError {
    #[error("model {model} is ASL-{asl_tier:?} which forbids autonomy level {level:?}")]
    AslLockViolation {
        model: String,
        asl_tier: AslTier,
        level: AutonomyLevel,
    },
    #[error("model {model} is EU AI Act high-risk and may not run above {cap:?}")]
    EuHighRiskCap { model: String, cap: AutonomyLevel },
}

/// D-341: ASL-3 or higher refuses any L5 request. EU high-risk models
/// are also capped at L3 per D-338.
pub fn validate_card(card: &CapabilityCard, level: AutonomyLevel) -> Result<(), AutonomyError> {
    if card.asl >= AslTier::Asl3 && level == AutonomyLevel::L5 {
        return Err(AutonomyError::AslLockViolation {
            model: card.model.clone(),
            asl_tier: card.asl,
            level,
        });
    }
    if card.eu_high_risk && level > AutonomyLevel::EU_HIGH_RISK_MAX {
        return Err(AutonomyError::EuHighRiskCap {
            model: card.model.clone(),
            cap: AutonomyLevel::EU_HIGH_RISK_MAX,
        });
    }
    Ok(())
}

// ----- D-342: level-scoped grants -----

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Grant {
    pub action_pattern: String,
    pub max_level: AutonomyLevel,
}

impl Grant {
    /// True iff a grant authorises action `pattern` at the given session
    /// level — both action pattern matches and `max_level` covers the
    /// current level.
    #[must_use]
    pub fn covers(&self, pattern: &str, current_level: AutonomyLevel) -> bool {
        self.action_pattern == pattern && current_level <= self.max_level
    }
}

// ----- D-343: autonomy event + signed consent ledger -----

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AutonomyEvent {
    pub event_id: String,
    pub level_before: AutonomyLevel,
    pub level_after: AutonomyLevel,
    pub principal_id: String,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LedgerEntry {
    pub event: AutonomyEvent,
    pub public_key: PublicKey,
    pub signature: SignatureBytes,
}

impl LedgerEntry {
    pub fn sign(sk: &PrivateKey, event: AutonomyEvent) -> Self {
        let canon = canonical_bytes(&event);
        let signature = sk.sign(&canon);
        Self {
            event,
            public_key: sk.public(),
            signature,
        }
    }

    pub fn verify(&self) -> bool {
        let canon = canonical_bytes(&self.event);
        self.public_key.verify(&canon, &self.signature).is_ok()
    }
}

fn canonical_bytes(event: &AutonomyEvent) -> Vec<u8> {
    serde_json::to_vec(event).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use harness_auth::generate_keypair;

    #[test]
    fn level_ordering_matches_numeric() {
        for w in AutonomyLevel::ALL.windows(2) {
            assert!(w[0] < w[1]);
            assert!(w[0].as_u8() < w[1].as_u8());
        }
    }

    #[test]
    fn approval_policy_is_strictest_at_l0_and_loosest_at_l5() {
        assert_eq!(policy_for(AutonomyLevel::L0), ApprovalPolicy::NoAct);
        assert_eq!(
            policy_for(AutonomyLevel::L1),
            ApprovalPolicy::PromptEveryAct
        );
        assert_eq!(policy_for(AutonomyLevel::L5), ApprovalPolicy::SilentAllow);
    }

    fn card(asl: AslTier, eu: bool) -> CapabilityCard {
        CapabilityCard {
            provider: "anthropic".into(),
            model: "claude".into(),
            asl,
            eu_high_risk: eu,
        }
    }

    #[test]
    fn asl3_at_l5_is_refused() {
        let err = validate_card(&card(AslTier::Asl3, false), AutonomyLevel::L5).unwrap_err();
        assert!(matches!(err, AutonomyError::AslLockViolation { .. }));
    }

    #[test]
    fn asl4_at_l5_is_also_refused() {
        let err = validate_card(&card(AslTier::Asl4, false), AutonomyLevel::L5).unwrap_err();
        assert!(matches!(err, AutonomyError::AslLockViolation { .. }));
    }

    #[test]
    fn asl3_at_l4_is_allowed() {
        validate_card(&card(AslTier::Asl3, false), AutonomyLevel::L4).unwrap();
    }

    #[test]
    fn eu_high_risk_caps_at_l3() {
        validate_card(&card(AslTier::Asl1, true), AutonomyLevel::L3).unwrap();
        let err = validate_card(&card(AslTier::Asl1, true), AutonomyLevel::L4).unwrap_err();
        assert!(matches!(err, AutonomyError::EuHighRiskCap { .. }));
    }

    #[test]
    fn grant_covers_only_within_max_level() {
        let g = Grant {
            action_pattern: "fs.read".into(),
            max_level: AutonomyLevel::L3,
        };
        assert!(g.covers("fs.read", AutonomyLevel::L2));
        assert!(g.covers("fs.read", AutonomyLevel::L3));
        assert!(!g.covers("fs.read", AutonomyLevel::L4));
        assert!(!g.covers("fs.write", AutonomyLevel::L1));
    }

    #[test]
    fn ledger_sign_then_verify_round_trips() {
        let (sk, _pk) = generate_keypair();
        let entry = LedgerEntry::sign(
            &sk,
            AutonomyEvent {
                event_id: "e1".into(),
                level_before: AutonomyLevel::L2,
                level_after: AutonomyLevel::L3,
                principal_id: "user-1".into(),
                created_at_ms: 1,
            },
        );
        assert!(entry.verify());
    }

    #[test]
    fn ledger_rejects_tampered_event() {
        let (sk, _pk) = generate_keypair();
        let mut entry = LedgerEntry::sign(
            &sk,
            AutonomyEvent {
                event_id: "e1".into(),
                level_before: AutonomyLevel::L2,
                level_after: AutonomyLevel::L3,
                principal_id: "user-1".into(),
                created_at_ms: 1,
            },
        );
        entry.event.level_after = AutonomyLevel::L5;
        assert!(!entry.verify());
    }
}
