// IMPLEMENTS: D-432
//! Self-Modification Gate. The path under `$HARNESS_REPO_ROOT` is
//! treated as META — even if the user has granted the project a
//! broad scope, editing Harness itself requires its own explicit
//! grant. METR's ARA threshold is the motivating finding: once a
//! model is competent enough to modify the agent runtime, every
//! modification must be a separate consent event.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelfModGate {
    pub harness_repo_root: PathBuf,
    pub grant: Option<SelfModGrant>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelfModGrant {
    pub principal_id: String,
    pub granted_at_iso: String,
    pub expires_at_iso: String,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SelfModError {
    #[error("self-modification refused: write to {path} requires an explicit grant")]
    NoGrant { path: PathBuf },
    #[error("self-modification grant for {principal} has expired")]
    Expired { principal: String },
}

pub fn evaluate_self_mod(
    gate: &SelfModGate,
    write_path: &Path,
    now_iso: &str,
) -> Result<(), SelfModError> {
    if !write_path.starts_with(&gate.harness_repo_root) {
        return Ok(());
    }
    let Some(grant) = &gate.grant else {
        return Err(SelfModError::NoGrant {
            path: write_path.to_path_buf(),
        });
    };
    if grant.expires_at_iso.as_str() <= now_iso {
        return Err(SelfModError::Expired {
            principal: grant.principal_id.clone(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gate(grant: Option<SelfModGrant>) -> SelfModGate {
        SelfModGate {
            harness_repo_root: PathBuf::from("/repos/harness"),
            grant,
        }
    }

    #[test]
    fn write_outside_repo_root_is_unaffected() {
        assert!(
            evaluate_self_mod(&gate(None), Path::new("/repos/other/file.rs"), "2026-04-22").is_ok()
        );
    }

    #[test]
    fn write_inside_repo_root_without_grant_refused() {
        let r = evaluate_self_mod(
            &gate(None),
            Path::new("/repos/harness/crates/harnessd/src/main.rs"),
            "2026-04-22",
        );
        assert!(matches!(r, Err(SelfModError::NoGrant { .. })));
    }

    #[test]
    fn valid_grant_passes() {
        let g = SelfModGrant {
            principal_id: "user-1".into(),
            granted_at_iso: "2026-04-22".into(),
            expires_at_iso: "2026-12-31".into(),
        };
        assert!(
            evaluate_self_mod(
                &gate(Some(g)),
                Path::new("/repos/harness/Cargo.toml"),
                "2026-04-22"
            )
            .is_ok()
        );
    }

    #[test]
    fn expired_grant_refused() {
        let g = SelfModGrant {
            principal_id: "user-1".into(),
            granted_at_iso: "2026-01-01".into(),
            expires_at_iso: "2026-04-01".into(),
        };
        let r = evaluate_self_mod(
            &gate(Some(g)),
            Path::new("/repos/harness/Cargo.toml"),
            "2026-04-22",
        );
        assert!(matches!(r, Err(SelfModError::Expired { .. })));
    }
}
