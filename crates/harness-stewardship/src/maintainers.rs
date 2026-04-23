// IMPLEMENTS: D-200
//! Co-maintainer admission gate. The next-step process can advance
//! only when at least two active maintainers each hold a Harmony
//! CLA 1.0 signature on file (D-220 lineage). The slate also rejects
//! a single-name list because that's the bus-factor we're trying to
//! escape.

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const CLA_VERSION: &str = "Harmony CLA 1.0";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MaintainerRecord {
    pub handle: String,
    pub active: bool,
    pub cla_signed_at_iso: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoMaintainerSlate {
    pub maintainers: Vec<MaintainerRecord>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum MaintainerError {
    #[error("co-maintainer slate has only {0} active maintainer(s); D-200 requires 2")]
    BelowMinimum(usize),
    #[error("co-maintainer {0} has no Harmony CLA 1.0 signature on file")]
    MissingCla(String),
}

pub fn evaluate_co_maintainers(slate: &CoMaintainerSlate) -> Result<(), MaintainerError> {
    let active: Vec<&MaintainerRecord> = slate.maintainers.iter().filter(|m| m.active).collect();
    if active.len() < 2 {
        return Err(MaintainerError::BelowMinimum(active.len()));
    }
    if let Some(missing) = active.iter().find(|m| m.cla_signed_at_iso.is_none()) {
        return Err(MaintainerError::MissingCla(missing.handle.clone()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(handle: &str, active: bool, signed: Option<&str>) -> MaintainerRecord {
        MaintainerRecord {
            handle: handle.into(),
            active,
            cla_signed_at_iso: signed.map(str::to_string),
        }
    }

    #[test]
    fn one_maintainer_below_minimum() {
        let s = CoMaintainerSlate {
            maintainers: vec![rec("a", true, Some("2026-01-01"))],
        };
        assert!(matches!(
            evaluate_co_maintainers(&s),
            Err(MaintainerError::BelowMinimum(1))
        ));
    }

    #[test]
    fn missing_cla_blocks() {
        let s = CoMaintainerSlate {
            maintainers: vec![rec("a", true, Some("2026-01-01")), rec("b", true, None)],
        };
        assert!(matches!(
            evaluate_co_maintainers(&s),
            Err(MaintainerError::MissingCla(_))
        ));
    }

    #[test]
    fn two_active_signed_passes() {
        let s = CoMaintainerSlate {
            maintainers: vec![
                rec("a", true, Some("2026-01-01")),
                rec("b", true, Some("2026-02-01")),
            ],
        };
        assert!(evaluate_co_maintainers(&s).is_ok());
    }

    #[test]
    fn inactive_does_not_count_toward_quorum() {
        let s = CoMaintainerSlate {
            maintainers: vec![
                rec("a", true, Some("2026-01-01")),
                rec("b", false, Some("2026-02-01")),
            ],
        };
        assert!(matches!(
            evaluate_co_maintainers(&s),
            Err(MaintainerError::BelowMinimum(1))
        ));
    }
}
