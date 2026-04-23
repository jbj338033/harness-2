// IMPLEMENTS: D-415
//! Dual-track contribution gate. Casual contributors sign DCO
//! (per-commit `Signed-off-by`); larger contributions opt into the
//! Harmony CLA 1.0 so we retain AGPL ↔ commercial dual-licensing
//! flexibility (D-196 lineage).

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContributionTrack {
    Dco,
    HarmonyCla1_0,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ClaError {
    #[error("DCO sign-off line missing on commit {0}")]
    MissingDcoSignoff(String),
    #[error("Harmony CLA 1.0 not on file for contributor {0}")]
    MissingHarmonyCla(String),
}

pub fn evaluate_contribution(
    track: ContributionTrack,
    commit_hash: &str,
    contributor_id: &str,
    has_dco_signoff: bool,
    harmony_cla_on_file: bool,
) -> Result<(), ClaError> {
    match track {
        ContributionTrack::Dco => {
            if !has_dco_signoff {
                return Err(ClaError::MissingDcoSignoff(commit_hash.to_string()));
            }
            Ok(())
        }
        ContributionTrack::HarmonyCla1_0 => {
            if !harmony_cla_on_file {
                return Err(ClaError::MissingHarmonyCla(contributor_id.to_string()));
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dco_track_passes_with_signoff() {
        assert!(evaluate_contribution(ContributionTrack::Dco, "abc", "user", true, false).is_ok());
    }

    #[test]
    fn dco_track_refuses_missing_signoff() {
        let r = evaluate_contribution(ContributionTrack::Dco, "abc", "user", false, false);
        assert!(matches!(r, Err(ClaError::MissingDcoSignoff(_))));
    }

    #[test]
    fn harmony_track_refuses_missing_cla() {
        let r = evaluate_contribution(ContributionTrack::HarmonyCla1_0, "abc", "user", true, false);
        assert!(matches!(r, Err(ClaError::MissingHarmonyCla(_))));
    }

    #[test]
    fn harmony_track_passes_with_cla() {
        assert!(
            evaluate_contribution(ContributionTrack::HarmonyCla1_0, "abc", "user", false, true,)
                .is_ok()
        );
    }
}
