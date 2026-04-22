// IMPLEMENTS: D-355
//! Citation lookup-only typestate. The `Unverified → Verified` lift
//! only succeeds when an external authority (CourtListener / Caselaw
//! Access Project) returns a matching record. The model is never
//! allowed to "vouch" for a citation it produced.

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CitationSource {
    CourtListener,
    CaselawAccessProject,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnverifiedCitation {
    pub raw: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifiedCitation {
    pub raw: String,
    pub source: CitationSource,
    pub canonical_id: String,
}

#[derive(Debug, Error)]
pub enum CitationError {
    #[error("citation lookup failed: {0}")]
    LookupFailed(String),
    #[error("citation does not match any record in {0:?}")]
    NotFound(CitationSource),
    #[error("citation is empty")]
    Empty,
}

impl UnverifiedCitation {
    pub fn new(raw: impl Into<String>) -> Result<Self, CitationError> {
        let raw = raw.into();
        if raw.trim().is_empty() {
            return Err(CitationError::Empty);
        }
        Ok(Self { raw })
    }

    /// Lift to `Verified` only if the lookup hit a real record. The
    /// caller supplies the lookup function so we can stub it in tests
    /// and swap providers per matter.
    pub fn verify<F>(self, lookup: F) -> Result<VerifiedCitation, CitationError>
    where
        F: FnOnce(&str) -> Result<(CitationSource, String), CitationError>,
    {
        let (source, canonical_id) = lookup(&self.raw)?;
        Ok(VerifiedCitation {
            raw: self.raw,
            source,
            canonical_id,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_citation_rejected() {
        assert!(matches!(
            UnverifiedCitation::new("   "),
            Err(CitationError::Empty)
        ));
    }

    #[test]
    fn verify_ok_promotes_to_verified() {
        let u = UnverifiedCitation::new("123 F.3d 456").unwrap();
        let v = u
            .verify(|s| {
                assert_eq!(s, "123 F.3d 456");
                Ok((CitationSource::CourtListener, "cl-001".into()))
            })
            .unwrap();
        assert_eq!(v.canonical_id, "cl-001");
        assert_eq!(v.source, CitationSource::CourtListener);
    }

    #[test]
    fn verify_not_found_keeps_value_unverified() {
        let u = UnverifiedCitation::new("Made Up v. Fake").unwrap();
        let r = u.verify(|_| Err(CitationError::NotFound(CitationSource::CourtListener)));
        assert!(matches!(r, Err(CitationError::NotFound(_))));
    }
}
