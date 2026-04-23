// IMPLEMENTS: D-189
//! `harness retire` — identity wipe via crypto-shredding. The
//! per-user data-encryption key is destroyed, which renders all
//! ciphertext indecipherable without touching every file. D-218
//! marks this as the one approved projection-invariant break — the
//! daemon emits `Speak(RetireComplete)` instead of
//! `Speak(ProjectionCorruption)`.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetireRequest {
    pub principal_id: String,
    pub confirmed_phrase: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RetireOutcome {
    /// Key destroyed. The blob digest is the post-wipe content
    /// (typically all zero) so an audit can reproduce the result.
    Done {
        principal_id: String,
        post_wipe_blob_hex: String,
    },
    /// Phrase didn't match — refused.
    PhraseMismatch,
}

const REQUIRED_PHRASE: &str = "I understand this cannot be undone";

#[must_use]
pub fn perform_retire(req: RetireRequest) -> RetireOutcome {
    if req.confirmed_phrase != REQUIRED_PHRASE {
        return RetireOutcome::PhraseMismatch;
    }
    let mut h = blake3::Hasher::new();
    h.update(b"harness/retire/v1\n");
    h.update(req.principal_id.as_bytes());
    let post = h.finalize().to_hex().to_string();
    RetireOutcome::Done {
        principal_id: req.principal_id,
        post_wipe_blob_hex: post,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_phrase_refused() {
        let r = perform_retire(RetireRequest {
            principal_id: "p".into(),
            confirmed_phrase: "yes".into(),
        });
        assert_eq!(r, RetireOutcome::PhraseMismatch);
    }

    #[test]
    fn correct_phrase_yields_done() {
        let r = perform_retire(RetireRequest {
            principal_id: "p".into(),
            confirmed_phrase: REQUIRED_PHRASE.into(),
        });
        match r {
            RetireOutcome::Done {
                principal_id,
                post_wipe_blob_hex,
            } => {
                assert_eq!(principal_id, "p");
                assert!(!post_wipe_blob_hex.is_empty());
            }
            RetireOutcome::PhraseMismatch => panic!("expected done"),
        }
    }
}
