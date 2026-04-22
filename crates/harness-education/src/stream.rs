// IMPLEMENTS: D-380
//! Ed25519-signed learner event stream. Every event a learner emits
//! is signed locally — there is no central server holding raw events.
//! A consuming dashboard reconstructs aggregates from the signed
//! stream and verifies each entry.

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LearnerEvent {
    pub learner_id: String,
    pub kind: String,
    pub payload: serde_json::Value,
    pub at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SignedEvent {
    pub event: LearnerEvent,
    pub public_key: [u8; 32],
    /// Ed25519 signature — 64 bytes. Stored as `Vec<u8>` because
    /// `serde` doesn't ship a `[u8; 64]` impl out of the box.
    pub signature: Vec<u8>,
}

#[derive(Debug, Error)]
pub enum SignError {
    #[error("event serialise: {0}")]
    Serialise(#[from] serde_json::Error),
    #[error("signature verify failed")]
    BadSignature,
    #[error("public key invalid")]
    BadKey,
}

pub fn sign_event(key: &SigningKey, event: LearnerEvent) -> Result<SignedEvent, SignError> {
    let bytes = canonical_bytes(&event)?;
    let sig: Signature = key.sign(&bytes);
    Ok(SignedEvent {
        event,
        public_key: key.verifying_key().to_bytes(),
        signature: sig.to_bytes().to_vec(),
    })
}

pub fn verify_event(signed: &SignedEvent) -> Result<(), SignError> {
    let vk = VerifyingKey::from_bytes(&signed.public_key).map_err(|_| SignError::BadKey)?;
    let bytes = canonical_bytes(&signed.event)?;
    let sig_arr: [u8; 64] = signed
        .signature
        .as_slice()
        .try_into()
        .map_err(|_| SignError::BadSignature)?;
    let sig = Signature::from_bytes(&sig_arr);
    vk.verify(&bytes, &sig).map_err(|_| SignError::BadSignature)
}

fn canonical_bytes(event: &LearnerEvent) -> Result<Vec<u8>, SignError> {
    let mut out = Vec::with_capacity(128);
    out.extend_from_slice(b"harness/edu/event/v1\n");
    out.extend_from_slice(event.learner_id.as_bytes());
    out.push(0);
    out.extend_from_slice(event.kind.as_bytes());
    out.push(0);
    out.extend_from_slice(&serde_json::to_vec(&event.payload)?);
    out.push(0);
    out.extend_from_slice(&event.at_ms.to_le_bytes());
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::OsRng;

    fn key() -> SigningKey {
        SigningKey::generate(&mut OsRng)
    }

    fn event() -> LearnerEvent {
        LearnerEvent {
            learner_id: "l1".into(),
            kind: "card_reviewed".into(),
            payload: serde_json::json!({"grade": 4}),
            at_ms: 1,
        }
    }

    #[test]
    fn signed_event_verifies() {
        let k = key();
        let s = sign_event(&k, event()).unwrap();
        assert!(verify_event(&s).is_ok());
    }

    #[test]
    fn tampered_payload_fails_verify() {
        let k = key();
        let mut s = sign_event(&k, event()).unwrap();
        s.event.payload = serde_json::json!({"grade": 5});
        assert!(matches!(verify_event(&s), Err(SignError::BadSignature)));
    }

    #[test]
    fn wrong_pubkey_fails_verify() {
        let k1 = key();
        let k2 = key();
        let mut s = sign_event(&k1, event()).unwrap();
        s.public_key = k2.verifying_key().to_bytes();
        assert!(matches!(verify_event(&s), Err(SignError::BadSignature)));
    }
}
