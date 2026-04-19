pub mod handshake;
pub mod tls;
pub mod unix;
pub mod ws;

pub use handshake::{
    AuthAttempt, AuthOutcome, Authenticator, HandshakeFrame, Nonce, hex_decode, hex_encode,
};
pub use tls::{TlsMaterials, fingerprint};
pub use unix::serve_unix;
pub use ws::{WsConfig, serve_ws};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum TransportError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("bind: {0}")]
    Bind(String),
    #[error("ws: {0}")]
    Ws(String),
}

pub type Result<T> = std::result::Result<T, TransportError>;
