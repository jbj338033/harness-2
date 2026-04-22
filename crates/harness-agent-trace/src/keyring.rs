// IMPLEMENTS: D-170, D-177
//! Multi-key trust store. Old keys stay around so traces signed before a
//! rotation still verify; the active key is the one used for new emit. Each
//! key gets a 16-byte deterministic id (D-177b) so a reader can pick the
//! right pubkey from the manifest's `key_id`.

use harness_auth::PublicKey;
use serde::{Deserialize, Serialize};

const KEY_ID_BYTES: usize = 16;
const KEY_ID_DOMAIN: &[u8] = b"harness/trace-key-id/v1";

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct KeyId(pub String);

impl KeyId {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[must_use]
pub fn key_id_for_public(pk: &PublicKey) -> KeyId {
    let mut hasher = blake3::Hasher::new();
    hasher.update(KEY_ID_DOMAIN);
    hasher.update(&pk.0);
    let digest = hasher.finalize();
    let bytes = &digest.as_bytes()[..KEY_ID_BYTES];
    KeyId(hex_encode(bytes))
}

fn hex_encode(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        write!(s, "{b:02x}").unwrap();
    }
    s
}

#[derive(Debug, Clone, Default)]
pub struct Keyring {
    active: Option<(KeyId, PublicKey)>,
    retired: Vec<(KeyId, PublicKey)>,
}

impl Keyring {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the key used for newly-signed traces. The previous active key
    /// (if any) is moved to `retired` so its old traces still verify.
    pub fn set_active(&mut self, pk: PublicKey) {
        let id = key_id_for_public(&pk);
        if let Some(prev) = self.active.take()
            && prev.0 != id
        {
            self.retired.push(prev);
        }
        self.active = Some((id, pk));
    }

    pub fn add_retired(&mut self, pk: PublicKey) {
        let id = key_id_for_public(&pk);
        if self.retired.iter().any(|(k, _)| k == &id) {
            return;
        }
        if self.active.as_ref().is_some_and(|(k, _)| k == &id) {
            return;
        }
        self.retired.push((id, pk));
    }

    #[must_use]
    pub fn active(&self) -> Option<&PublicKey> {
        self.active.as_ref().map(|(_, pk)| pk)
    }

    #[must_use]
    pub fn active_id(&self) -> Option<&KeyId> {
        self.active.as_ref().map(|(id, _)| id)
    }

    /// Look up a pubkey by id — checks active first, then retired.
    #[must_use]
    pub fn lookup(&self, id: &str) -> Option<&PublicKey> {
        if let Some((aid, pk)) = self.active.as_ref()
            && aid.0 == id
        {
            return Some(pk);
        }
        self.retired
            .iter()
            .find(|(rid, _)| rid.0 == id)
            .map(|(_, pk)| pk)
    }

    #[must_use]
    pub fn known_count(&self) -> usize {
        usize::from(self.active.is_some()) + self.retired.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use harness_auth::generate_keypair;

    #[test]
    fn key_id_is_32_hex_chars() {
        let (_, pk) = generate_keypair();
        let id = key_id_for_public(&pk);
        assert_eq!(id.as_str().len(), 32);
        assert!(id.as_str().chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn key_id_is_deterministic_per_pubkey() {
        let (_, pk) = generate_keypair();
        assert_eq!(key_id_for_public(&pk), key_id_for_public(&pk));
    }

    #[test]
    fn distinct_keys_get_distinct_ids() {
        let (_, p1) = generate_keypair();
        let (_, p2) = generate_keypair();
        assert_ne!(key_id_for_public(&p1), key_id_for_public(&p2));
    }

    #[test]
    fn rotating_active_retires_the_previous() {
        let (_, p1) = generate_keypair();
        let (_, p2) = generate_keypair();
        let mut ring = Keyring::new();
        ring.set_active(p1.clone());
        ring.set_active(p2.clone());
        assert_eq!(ring.active(), Some(&p2));
        // p1 should now be looked up via retired
        let id1 = key_id_for_public(&p1);
        assert_eq!(ring.lookup(id1.as_str()), Some(&p1));
        assert_eq!(ring.known_count(), 2);
    }

    #[test]
    fn re_setting_same_active_does_not_grow_retired() {
        let (_, p1) = generate_keypair();
        let mut ring = Keyring::new();
        ring.set_active(p1.clone());
        ring.set_active(p1.clone());
        assert_eq!(ring.known_count(), 1);
    }

    #[test]
    fn add_retired_is_idempotent() {
        let (_, p1) = generate_keypair();
        let mut ring = Keyring::new();
        ring.add_retired(p1.clone());
        ring.add_retired(p1.clone());
        assert_eq!(ring.known_count(), 1);
    }

    #[test]
    fn lookup_misses_unknown_id() {
        let ring = Keyring::new();
        assert!(ring.lookup("0000000000000000").is_none());
    }
}
