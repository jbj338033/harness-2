use crate::key::{PublicKey, SignatureBytes};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use thiserror::Error;

pub const NONCE_TTL: Duration = Duration::from_secs(60);

#[derive(Debug, Error)]
pub enum ChallengeError {
    #[error("nonce expired or malformed")]
    NonceInvalid,
    #[error("signature does not match public key")]
    BadSignature,
    #[error("clock skew out of bounds")]
    Clock,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Nonce {
    pub value: [u8; 32],
    pub issued_at: i64,
}

impl Nonce {
    #[must_use]
    pub fn signing_bytes(&self) -> [u8; 40] {
        let mut out = [0u8; 40];
        out[..32].copy_from_slice(&self.value);
        out[32..].copy_from_slice(&self.issued_at.to_le_bytes());
        out
    }
}

#[must_use]
pub fn issue_nonce() -> Nonce {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    let issued_at = i64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis(),
    )
    .unwrap_or(0);
    Nonce {
        value: bytes,
        issued_at,
    }
}

pub fn verify_signature(
    nonce: &Nonce,
    public_key: &PublicKey,
    signature: &SignatureBytes,
) -> Result<(), ChallengeError> {
    let now = i64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis(),
    )
    .map_err(|_| ChallengeError::Clock)?;
    let age = now.saturating_sub(nonce.issued_at);
    if age < 0 || age > i64::try_from(NONCE_TTL.as_millis()).unwrap() {
        return Err(ChallengeError::NonceInvalid);
    }
    let msg = nonce.signing_bytes();
    public_key
        .verify(&msg, signature)
        .map_err(|_| ChallengeError::BadSignature)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::key::generate_keypair;

    #[test]
    fn fresh_nonce_verifies() {
        let n = issue_nonce();
        let (sk, pk) = generate_keypair();
        let sig = sk.sign(&n.signing_bytes());
        assert!(verify_signature(&n, &pk, &sig).is_ok());
    }

    #[test]
    fn wrong_key_rejected() {
        let n = issue_nonce();
        let (sk, _) = generate_keypair();
        let (_, pk2) = generate_keypair();
        let sig = sk.sign(&n.signing_bytes());
        assert!(matches!(
            verify_signature(&n, &pk2, &sig),
            Err(ChallengeError::BadSignature)
        ));
    }

    #[test]
    fn stale_nonce_rejected() {
        let mut n = issue_nonce();
        n.issued_at -= 120_000;
        let (sk, pk) = generate_keypair();
        let sig = sk.sign(&n.signing_bytes());
        assert!(matches!(
            verify_signature(&n, &pk, &sig),
            Err(ChallengeError::NonceInvalid)
        ));
    }

    #[test]
    fn tampered_nonce_fails_verification() {
        let n = issue_nonce();
        let (sk, pk) = generate_keypair();
        let sig = sk.sign(&n.signing_bytes());
        let mut n2 = n.clone();
        n2.value[0] ^= 0xFF;
        assert!(matches!(
            verify_signature(&n2, &pk, &sig),
            Err(ChallengeError::BadSignature)
        ));
    }

    #[test]
    fn signing_bytes_are_deterministic() {
        let n = Nonce {
            value: [42u8; 32],
            issued_at: 1_700_000_000_000,
        };
        assert_eq!(n.signing_bytes(), n.signing_bytes());
    }
}
