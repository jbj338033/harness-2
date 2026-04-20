use async_trait::async_trait;
use harness_auth::{
    challenge::{Nonce as AuthNonce, verify_signature},
    key::{PublicKey, SignatureBytes},
    pairing::{PairingSession, register_device},
};
use harness_storage::{ReaderPool, WriterHandle};
use harness_transport::{AuthAttempt, AuthOutcome, Authenticator, Nonce};
use std::sync::Arc;
use tracing::{debug, warn};

pub struct DeviceAuthenticator {
    writer: WriterHandle,
    readers: Arc<ReaderPool>,
    pairing: PairingSession,
}

impl DeviceAuthenticator {
    #[must_use]
    pub fn new(
        writer: WriterHandle,
        readers: Arc<ReaderPool>,
        pairing: PairingSession,
    ) -> Arc<Self> {
        Arc::new(Self {
            writer,
            readers,
            pairing,
        })
    }
}

#[async_trait]
impl Authenticator for DeviceAuthenticator {
    async fn verify(&self, nonce: Nonce, attempt: AuthAttempt) -> AuthOutcome {
        let auth_nonce = AuthNonce {
            value: nonce.value,
            issued_at: nonce.issued_at,
        };

        match attempt {
            AuthAttempt::Existing {
                public_key,
                signature,
            } => {
                let pk = PublicKey(public_key);
                let sig = SignatureBytes(signature);
                if verify_signature(&auth_nonce, &pk, &sig).is_err() {
                    return AuthOutcome::Rejected {
                        reason: "signature does not match nonce".into(),
                    };
                }
                match lookup_device(&self.readers, &pk) {
                    Ok(Some(id)) => AuthOutcome::Accepted { device_id: id },
                    Ok(None) => AuthOutcome::Rejected {
                        reason: "unknown public key — pair this device first".into(),
                    },
                    Err(e) => {
                        warn!(error = %e, "device lookup failed");
                        AuthOutcome::Rejected {
                            reason: "internal storage error".into(),
                        }
                    }
                }
            }
            AuthAttempt::Pairing {
                code,
                name,
                public_key,
                signature,
            } => {
                let pk = PublicKey(public_key);
                let sig = SignatureBytes(signature);
                if verify_signature(&auth_nonce, &pk, &sig).is_err() {
                    return AuthOutcome::Rejected {
                        reason: "signature does not match nonce".into(),
                    };
                }
                let notifier = match self.pairing.consume(&code) {
                    Ok(n) => n,
                    Err(e) => {
                        debug!(error = %e, "pairing code rejected");
                        return AuthOutcome::Rejected {
                            reason: "invalid or expired pairing code".into(),
                        };
                    }
                };
                match register_device(&self.writer, name.clone(), pk.clone()).await {
                    Ok(rec) => {
                        let device_id = rec.id.clone();
                        notifier.fulfill(rec.id, rec.name, pk);
                        AuthOutcome::Accepted { device_id }
                    }
                    Err(e) => {
                        warn!(error = %e, %name, "register_device failed");
                        drop(notifier);
                        AuthOutcome::Rejected {
                            reason: e.to_string(),
                        }
                    }
                }
            }
        }
    }
}

fn lookup_device(readers: &ReaderPool, pk: &PublicKey) -> harness_storage::Result<Option<String>> {
    let reader = readers.get()?;
    let mut stmt = reader.prepare("SELECT id FROM devices WHERE public_key = ?1")?;
    let mut rows = stmt.query([pk.0.to_vec()])?;
    if let Some(row) = rows.next()? {
        let id: String = row.get(0)?;
        Ok(Some(id))
    } else {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use harness_auth::key::generate_keypair;
    use harness_storage::{Database, Writer};
    use tempfile::NamedTempFile;

    fn setup() -> (NamedTempFile, WriterHandle, Arc<ReaderPool>) {
        let f = NamedTempFile::new().unwrap();
        Database::open(f.path()).unwrap();
        let w = Writer::spawn(f.path()).unwrap();
        let readers = Arc::new(ReaderPool::with_defaults(f.path().to_path_buf()));
        (f, w, readers)
    }

    #[tokio::test]
    async fn unknown_key_is_rejected() {
        let (_f, w, readers) = setup();
        let auth = DeviceAuthenticator::new(w, readers, PairingSession::default());
        let (sk, pk) = generate_keypair();
        let nonce = Nonce {
            value: [1u8; 32],
            issued_at: harness_core::now().as_millis(),
        };
        let sig = sk.sign(&auth_signing_bytes(&nonce));
        let outcome = auth
            .verify(
                nonce,
                AuthAttempt::Existing {
                    public_key: pk.0,
                    signature: sig.0,
                },
            )
            .await;
        assert!(matches!(outcome, AuthOutcome::Rejected { .. }));
    }

    #[tokio::test]
    async fn bad_signature_is_rejected() {
        let (_f, w, readers) = setup();
        let auth = DeviceAuthenticator::new(w, readers, PairingSession::default());
        let (_sk, pk) = generate_keypair();
        let nonce = Nonce {
            value: [2u8; 32],
            issued_at: harness_core::now().as_millis(),
        };
        let outcome = auth
            .verify(
                nonce,
                AuthAttempt::Existing {
                    public_key: pk.0,
                    signature: [0u8; 64],
                },
            )
            .await;
        assert!(matches!(outcome, AuthOutcome::Rejected { .. }));
    }

    #[tokio::test]
    async fn pairing_registers_and_subsequent_auth_accepts() {
        let (_f, w, readers) = setup();
        let pairing = PairingSession::default();
        let code = pairing.new_code();
        let auth = DeviceAuthenticator::new(w, readers, pairing);

        let (sk, pk) = generate_keypair();
        let nonce1 = Nonce {
            value: [3u8; 32],
            issued_at: harness_core::now().as_millis(),
        };
        let sig1 = sk.sign(&auth_signing_bytes(&nonce1));
        let outcome = auth
            .verify(
                nonce1,
                AuthAttempt::Pairing {
                    code,
                    name: "laptop".into(),
                    public_key: pk.0,
                    signature: sig1.0,
                },
            )
            .await;
        match outcome {
            AuthOutcome::Accepted { .. } => {}
            AuthOutcome::Rejected { reason } => panic!("pairing rejected: {reason}"),
        }

        let nonce2 = Nonce {
            value: [4u8; 32],
            issued_at: harness_core::now().as_millis(),
        };
        let sig2 = sk.sign(&auth_signing_bytes(&nonce2));
        let outcome = auth
            .verify(
                nonce2,
                AuthAttempt::Existing {
                    public_key: pk.0,
                    signature: sig2.0,
                },
            )
            .await;
        assert!(matches!(outcome, AuthOutcome::Accepted { .. }));
    }

    fn auth_signing_bytes(nonce: &Nonce) -> [u8; 40] {
        let mut out = [0u8; 40];
        out[..32].copy_from_slice(&nonce.value);
        out[32..].copy_from_slice(&nonce.issued_at.to_le_bytes());
        out
    }
}
