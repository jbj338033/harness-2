//! Crypto-shredding (GDPR Art 17 right-to-erasure).
//!
//! Each data subject's bytes-at-rest are encrypted with a
//! per-subject Data Encryption Key (DEK). On erasure request we
//! destroy the DEK; the ciphertext on disk becomes mathematically
//! unrecoverable without touching every file. We record a `ShredProof`
//! that names the subject and the post-shred DEK fingerprint (which
//! must hash to the all-zero key) so an audit can prove the wipe.
//!
//! This is the full version of D-189 retire — D-189 is the
//! single-user CLI; this module is the API the daemon uses for any
//! Art 17 request.

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DataEncryptionKey {
    pub subject_id: String,
    pub key_bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShredRequest {
    pub subject_id: String,
    pub legal_basis: ShredLegalBasis,
    pub at_iso: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShredLegalBasis {
    GdprArt17,
    KrPipa36,
    UserRequest,
    RetentionExpiry,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShredProof {
    pub subject_id: String,
    pub legal_basis: ShredLegalBasis,
    pub post_shred_key_fingerprint_hex: String,
    pub at_iso: String,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ShredError {
    #[error("subject mismatch: dek for {dek}, request for {req}")]
    SubjectMismatch { dek: String, req: String },
    #[error("legal basis missing")]
    MissingBasis,
}

/// Destroys the DEK in place by zeroing every byte and returns the
/// proof. The caller is expected to drop the now-zero key after
/// recording the proof.
pub fn shred(dek: &mut DataEncryptionKey, req: &ShredRequest) -> Result<ShredProof, ShredError> {
    if dek.subject_id != req.subject_id {
        return Err(ShredError::SubjectMismatch {
            dek: dek.subject_id.clone(),
            req: req.subject_id.clone(),
        });
    }
    if req.at_iso.trim().is_empty() {
        return Err(ShredError::MissingBasis);
    }
    for byte in &mut dek.key_bytes {
        *byte = 0;
    }
    let fingerprint = key_fingerprint(&dek.key_bytes);
    Ok(ShredProof {
        subject_id: req.subject_id.clone(),
        legal_basis: req.legal_basis,
        post_shred_key_fingerprint_hex: fingerprint,
        at_iso: req.at_iso.clone(),
    })
}

#[must_use]
pub fn key_fingerprint(key_bytes: &[u8]) -> String {
    let mut h = blake3::Hasher::new();
    h.update(b"harness/shred/dek/v1\n");
    h.update(key_bytes);
    h.finalize().to_hex().to_string()
}

#[must_use]
pub fn zero_key_fingerprint(len: usize) -> String {
    let zero = vec![0u8; len];
    key_fingerprint(&zero)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dek() -> DataEncryptionKey {
        DataEncryptionKey {
            subject_id: "user-1".into(),
            key_bytes: vec![1u8; 32],
        }
    }

    fn req() -> ShredRequest {
        ShredRequest {
            subject_id: "user-1".into(),
            legal_basis: ShredLegalBasis::GdprArt17,
            at_iso: "2026-04-22T10:00:00Z".into(),
        }
    }

    #[test]
    fn shred_zeros_key_and_returns_proof() {
        let mut k = dek();
        let proof = shred(&mut k, &req()).unwrap();
        assert!(k.key_bytes.iter().all(|b| *b == 0));
        assert_eq!(proof.subject_id, "user-1");
        assert_eq!(
            proof.post_shred_key_fingerprint_hex,
            zero_key_fingerprint(32)
        );
    }

    #[test]
    fn subject_mismatch_refused() {
        let mut k = dek();
        let mut r = req();
        r.subject_id = "other".into();
        assert!(matches!(
            shred(&mut k, &r),
            Err(ShredError::SubjectMismatch { .. })
        ));
        assert!(k.key_bytes.iter().any(|b| *b != 0));
    }

    #[test]
    fn missing_iso_refused() {
        let mut k = dek();
        let mut r = req();
        r.at_iso = "  ".into();
        assert!(matches!(shred(&mut k, &r), Err(ShredError::MissingBasis)));
    }

    #[test]
    fn fingerprint_changes_when_key_changes() {
        let a = key_fingerprint(b"abc");
        let b = key_fingerprint(b"def");
        assert_ne!(a, b);
    }

    #[test]
    fn legal_basis_round_trips_via_serde() {
        for b in [
            ShredLegalBasis::GdprArt17,
            ShredLegalBasis::KrPipa36,
            ShredLegalBasis::UserRequest,
            ShredLegalBasis::RetentionExpiry,
        ] {
            let s = serde_json::to_string(&b).unwrap();
            let back: ShredLegalBasis = serde_json::from_str(&s).unwrap();
            assert_eq!(back, b);
        }
    }
}
