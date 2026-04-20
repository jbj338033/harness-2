pub mod challenge;
pub mod key;
pub mod oauth;
pub mod pairing;

pub use challenge::{ChallengeError, Nonce, issue_nonce, verify_signature};
pub use key::{PrivateKey, PublicKey, SignatureBytes, generate_keypair};
pub use pairing::{
    DeviceRecord, Notifier, PairOutcome, PairingError, PairingSession, list_devices,
    register_device, revoke_device,
};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("storage: {0}")]
    Storage(#[from] harness_storage::StorageError),
    #[error("pairing: {0}")]
    Pairing(#[from] PairingError),
    #[error("challenge: {0}")]
    Challenge(#[from] ChallengeError),
}
