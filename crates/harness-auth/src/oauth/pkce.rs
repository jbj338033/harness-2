use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use rand::RngCore;
use sha2::{Digest, Sha256};

#[derive(Debug, Clone)]
pub struct Pkce {
    pub verifier: String,
    pub challenge: String,
    pub state: String,
}

#[must_use]
pub fn gen_pkce() -> Pkce {
    let mut rng = rand::thread_rng();
    let mut verifier_bytes = [0u8; 32];
    rng.fill_bytes(&mut verifier_bytes);
    let verifier = URL_SAFE_NO_PAD.encode(verifier_bytes);

    let digest = Sha256::digest(verifier.as_bytes());
    let challenge = URL_SAFE_NO_PAD.encode(digest);

    let mut state_bytes = [0u8; 16];
    rng.fill_bytes(&mut state_bytes);
    let state = hex::encode(state_bytes);

    Pkce {
        verifier,
        challenge,
        state,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verifier_length_is_in_spec() {
        let p = gen_pkce();
        assert_eq!(p.verifier.len(), 43);
    }

    #[test]
    fn challenge_matches_sha256_of_verifier() {
        let p = gen_pkce();
        let expected = URL_SAFE_NO_PAD.encode(Sha256::digest(p.verifier.as_bytes()));
        assert_eq!(p.challenge, expected);
    }

    #[test]
    fn state_is_hex() {
        let p = gen_pkce();
        assert_eq!(p.state.len(), 32);
        assert!(p.state.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn two_triples_differ() {
        let a = gen_pkce();
        let b = gen_pkce();
        assert_ne!(a.verifier, b.verifier);
        assert_ne!(a.state, b.state);
    }
}
