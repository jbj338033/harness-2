// IMPLEMENTS: D-420
//! Reproducible build + 2-of-2 detached signature gate, SLSA L3
//! attested. The actual signing pipeline lives in
//! `scripts/release-sign.sh`; this module decides whether a release
//! has the signatures and SLSA attestation needed to ship.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use thiserror::Error;

pub const REQUIRED_SIGNERS: u8 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SlsaLevel {
    L0,
    L1,
    L2,
    L3,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseSignature {
    pub artifact_hex: String,
    pub signers: BTreeSet<String>,
    pub slsa_level: SlsaLevel,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ReleaseError {
    #[error("release missing artifact digest")]
    MissingArtifact,
    #[error("release has {got} signers, requires {required}")]
    NotEnoughSigners { got: u8, required: u8 },
    #[error("release SLSA level {got:?} below required L3")]
    SlsaTooLow { got: SlsaLevel },
}

pub fn evaluate_release(sig: &ReleaseSignature) -> Result<(), ReleaseError> {
    if sig.artifact_hex.trim().is_empty() {
        return Err(ReleaseError::MissingArtifact);
    }
    let signer_count = u8::try_from(sig.signers.len()).unwrap_or(u8::MAX);
    if signer_count < REQUIRED_SIGNERS {
        return Err(ReleaseError::NotEnoughSigners {
            got: signer_count,
            required: REQUIRED_SIGNERS,
        });
    }
    if sig.slsa_level < SlsaLevel::L3 {
        return Err(ReleaseError::SlsaTooLow {
            got: sig.slsa_level,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sig(signers: &[&str], slsa: SlsaLevel) -> ReleaseSignature {
        ReleaseSignature {
            artifact_hex: "abc".into(),
            signers: signers.iter().map(|s| (*s).to_string()).collect(),
            slsa_level: slsa,
        }
    }

    #[test]
    fn full_release_passes() {
        assert!(evaluate_release(&sig(&["alice", "bob"], SlsaLevel::L3)).is_ok());
    }

    #[test]
    fn one_signer_refused() {
        let r = evaluate_release(&sig(&["alice"], SlsaLevel::L3));
        assert!(matches!(r, Err(ReleaseError::NotEnoughSigners { .. })));
    }

    #[test]
    fn slsa_l2_refused() {
        let r = evaluate_release(&sig(&["alice", "bob"], SlsaLevel::L2));
        assert!(matches!(r, Err(ReleaseError::SlsaTooLow { .. })));
    }

    #[test]
    fn missing_artifact_refused() {
        let mut s = sig(&["alice", "bob"], SlsaLevel::L3);
        s.artifact_hex.clear();
        assert_eq!(evaluate_release(&s), Err(ReleaseError::MissingArtifact));
    }
}
