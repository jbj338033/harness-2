// IMPLEMENTS: D-371
//! Citation-forced + numeric integrity. Every numeric claim that
//! crosses the daemon boundary must carry a citation handle AND must
//! survive a round-trip against its source value (within tolerance).
//! D-355 (legal citation) and D-294 (numeric integrity) feed this
//! same enforcer.

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NumericClaim {
    pub citation_id: String,
    pub claimed_value: f64,
    pub unit: String,
}

#[derive(Debug, Error)]
pub enum NumericIntegrityError {
    #[error("citation id is empty — cannot publish a numeric claim without a source")]
    MissingCitation,
    #[error("claimed value {claimed} does not match source {source_value} (tolerance {tol})")]
    ValueMismatch {
        claimed: f64,
        source_value: f64,
        tol: f64,
    },
    #[error("claimed unit {claimed:?} differs from source unit {source_unit:?}")]
    UnitMismatch {
        claimed: String,
        source_unit: String,
    },
}

pub fn verify_numeric_claim(
    claim: &NumericClaim,
    source_value: f64,
    source_unit: &str,
    tol: f64,
) -> Result<(), NumericIntegrityError> {
    if claim.citation_id.trim().is_empty() {
        return Err(NumericIntegrityError::MissingCitation);
    }
    if claim.unit != source_unit {
        return Err(NumericIntegrityError::UnitMismatch {
            claimed: claim.unit.clone(),
            source_unit: source_unit.to_string(),
        });
    }
    if (claim.claimed_value - source_value).abs() > tol {
        return Err(NumericIntegrityError::ValueMismatch {
            claimed: claim.claimed_value,
            source_value,
            tol,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn claim(value: f64, unit: &str, cite: &str) -> NumericClaim {
        NumericClaim {
            citation_id: cite.into(),
            claimed_value: value,
            unit: unit.into(),
        }
    }

    #[test]
    fn missing_citation_rejected() {
        let r = verify_numeric_claim(&claim(1.0, "USD", ""), 1.0, "USD", 0.01);
        assert!(matches!(r, Err(NumericIntegrityError::MissingCitation)));
    }

    #[test]
    fn unit_mismatch_rejected() {
        let r = verify_numeric_claim(&claim(1.0, "USD", "10-K-2025"), 1.0, "EUR", 0.01);
        assert!(matches!(r, Err(NumericIntegrityError::UnitMismatch { .. })));
    }

    #[test]
    fn value_outside_tol_rejected() {
        let r = verify_numeric_claim(&claim(1.5, "USD", "10-K"), 1.0, "USD", 0.01);
        assert!(matches!(
            r,
            Err(NumericIntegrityError::ValueMismatch { .. })
        ));
    }

    #[test]
    fn value_inside_tol_passes() {
        let r = verify_numeric_claim(&claim(1.0001, "USD", "10-K"), 1.0, "USD", 0.001);
        assert!(r.is_ok());
    }
}
