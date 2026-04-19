use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy)]
pub struct Nonce {
    pub value: [u8; 32],
    pub issued_at: i64,
}

#[derive(Debug, Clone)]
pub enum AuthAttempt {
    Existing {
        public_key: [u8; 32],
        signature: [u8; 64],
    },
    Pairing {
        code: String,
        name: String,
        public_key: [u8; 32],
        signature: [u8; 64],
    },
}

#[derive(Debug, Clone)]
pub enum AuthOutcome {
    Accepted { device_id: String },
    Rejected { reason: String },
}

#[async_trait]
pub trait Authenticator: Send + Sync {
    async fn verify(&self, nonce: Nonce, attempt: AuthAttempt) -> AuthOutcome;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HandshakeFrame {
    Challenge {
        nonce: String,
        issued_at: i64,
    },
    Auth {
        public_key: String,
        signature: String,
    },
    Pair {
        code: String,
        name: String,
        public_key: String,
        signature: String,
    },
    Welcome {
        device_id: String,
    },
    Reject {
        reason: String,
    },
}

#[must_use]
pub fn hex_encode(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        write!(&mut s, "{b:02x}").unwrap();
    }
    s
}

pub fn hex_decode<const N: usize>(s: &str) -> Result<[u8; N], HexError> {
    if s.len() != N * 2 {
        return Err(HexError::Length {
            expected: N * 2,
            actual: s.len(),
        });
    }
    let mut out = [0u8; N];
    for (i, slot) in out.iter_mut().enumerate() {
        let hi = decode_nibble(s.as_bytes()[i * 2])?;
        let lo = decode_nibble(s.as_bytes()[i * 2 + 1])?;
        *slot = (hi << 4) | lo;
    }
    Ok(out)
}

fn decode_nibble(b: u8) -> Result<u8, HexError> {
    match b {
        b'0'..=b'9' => Ok(b - b'0'),
        b'a'..=b'f' => Ok(b - b'a' + 10),
        b'A'..=b'F' => Ok(b - b'A' + 10),
        other => Err(HexError::Invalid(other as char)),
    }
}

#[derive(Debug, thiserror::Error)]
pub enum HexError {
    #[error("hex length mismatch: expected {expected}, got {actual}")]
    Length { expected: usize, actual: usize },
    #[error("invalid hex character: {0}")]
    Invalid(char),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_roundtrip() {
        let bytes: [u8; 4] = [0xde, 0xad, 0xbe, 0xef];
        let s = hex_encode(&bytes);
        assert_eq!(s, "deadbeef");
        let back: [u8; 4] = hex_decode(&s).unwrap();
        assert_eq!(back, bytes);
    }

    #[test]
    fn hex_rejects_bad_length() {
        let err = hex_decode::<4>("abc").unwrap_err();
        assert!(matches!(err, HexError::Length { .. }));
    }

    #[test]
    fn hex_rejects_non_hex_char() {
        let err = hex_decode::<1>("zz").unwrap_err();
        assert!(matches!(err, HexError::Invalid(_)));
    }

    #[test]
    fn handshake_frame_roundtrips() {
        let c = HandshakeFrame::Challenge {
            nonce: "ab".repeat(32),
            issued_at: 1,
        };
        let s = serde_json::to_string(&c).unwrap();
        let back: HandshakeFrame = serde_json::from_str(&s).unwrap();
        match back {
            HandshakeFrame::Challenge { issued_at, .. } => assert_eq!(issued_at, 1),
            _ => panic!("wrong variant"),
        }
    }
}
