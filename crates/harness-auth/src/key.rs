use ed25519_dalek::{Signer, SigningKey, VerifyingKey};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicKey(pub [u8; 32]);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignatureBytes(pub [u8; 64]);

impl Serialize for SignatureBytes {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_bytes(&self.0)
    }
}

impl<'de> Deserialize<'de> for SignatureBytes {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let v = <Vec<u8>>::deserialize(d)?;
        if v.len() != 64 {
            return Err(serde::de::Error::custom(format!(
                "expected 64 bytes, got {}",
                v.len()
            )));
        }
        let mut arr = [0u8; 64];
        arr.copy_from_slice(&v);
        Ok(Self(arr))
    }
}

pub struct PrivateKey(SigningKey);

impl Drop for PrivateKey {
    fn drop(&mut self) {
        let mut bytes = self.0.to_bytes();
        bytes.zeroize();
    }
}

impl PrivateKey {
    #[must_use]
    pub fn from_bytes(b: &[u8; 32]) -> Self {
        Self(SigningKey::from_bytes(b))
    }

    #[must_use]
    pub fn sign(&self, msg: &[u8]) -> SignatureBytes {
        SignatureBytes(self.0.sign(msg).to_bytes())
    }

    #[must_use]
    pub fn public(&self) -> PublicKey {
        PublicKey(self.0.verifying_key().to_bytes())
    }

    #[must_use]
    pub fn to_bytes(&self) -> [u8; 32] {
        self.0.to_bytes()
    }
}

impl PublicKey {
    pub fn verify(
        &self,
        msg: &[u8],
        sig: &SignatureBytes,
    ) -> Result<(), ed25519_dalek::ed25519::Error> {
        let vk = VerifyingKey::from_bytes(&self.0)?;
        let signature = ed25519_dalek::Signature::from_bytes(&sig.0);
        vk.verify_strict(msg, &signature)
    }
}

#[must_use]
pub fn generate_keypair() -> (PrivateKey, PublicKey) {
    let signing = SigningKey::generate(&mut OsRng);
    let public = PublicKey(signing.verifying_key().to_bytes());
    (PrivateKey(signing), public)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_and_sign_roundtrip() {
        let (sk, pk) = generate_keypair();
        let msg = b"hello harness";
        let sig = sk.sign(msg);
        assert!(pk.verify(msg, &sig).is_ok());
    }

    #[test]
    fn verification_rejects_tampered_message() {
        let (sk, pk) = generate_keypair();
        let sig = sk.sign(b"original");
        assert!(pk.verify(b"tampered", &sig).is_err());
    }

    #[test]
    fn verification_rejects_wrong_key() {
        let (sk1, _) = generate_keypair();
        let (_, pk2) = generate_keypair();
        let sig = sk1.sign(b"m");
        assert!(pk2.verify(b"m", &sig).is_err());
    }

    #[test]
    fn roundtrip_from_bytes() {
        let (sk, pk) = generate_keypair();
        let raw = sk.to_bytes();
        let restored = PrivateKey::from_bytes(&raw);
        assert_eq!(restored.public(), pk);
    }

    #[test]
    fn public_key_is_serde() {
        let (_, pk) = generate_keypair();
        let s = serde_json::to_string(&pk).unwrap();
        let back: PublicKey = serde_json::from_str(&s).unwrap();
        assert_eq!(pk, back);
    }
}
