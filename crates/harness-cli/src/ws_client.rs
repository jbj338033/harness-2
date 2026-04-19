use anyhow::{Result, anyhow, bail};
use futures::{SinkExt, StreamExt};
use harness_auth::key::{PrivateKey, PublicKey};
use harness_transport::handshake::{HandshakeFrame, hex_encode};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, Error as TlsError, SignatureScheme};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::time::timeout;
use tokio_tungstenite::Connector;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::protocol::Message;

pub struct PairOutcome {
    pub device_id: String,
    pub fingerprint: String,
}

pub async fn pair(
    url: &str,
    code: &str,
    name: &str,
    private_key: &PrivateKey,
    public_key: &PublicKey,
    expected_fingerprint: Option<&str>,
) -> Result<PairOutcome> {
    rustls::crypto::ring::default_provider()
        .install_default()
        .ok();

    let seen: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let verifier = Arc::new(PinVerifier {
        expected: expected_fingerprint.map(str::to_string),
        seen: seen.clone(),
    });
    let config = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(verifier)
        .with_no_client_auth();

    let request = url
        .into_client_request()
        .map_err(|e| anyhow!("invalid url: {e}"))?;
    let (ws, _resp) = tokio_tungstenite::connect_async_tls_with_config(
        request,
        None,
        false,
        Some(Connector::Rustls(Arc::new(config))),
    )
    .await
    .map_err(|e| anyhow!("ws connect: {e}"))?;
    let (mut tx, mut rx) = ws.split();

    let frame = next_frame(&mut rx).await?;
    let HandshakeFrame::Challenge { nonce, issued_at } = frame else {
        bail!("expected Challenge frame");
    };
    let nonce_bytes = hex_decode32(&nonce)?;
    let mut signing_bytes = [0u8; 40];
    signing_bytes[..32].copy_from_slice(&nonce_bytes);
    signing_bytes[32..].copy_from_slice(&issued_at.to_le_bytes());
    let signature = private_key.sign(&signing_bytes);

    let pair_frame = HandshakeFrame::Pair {
        code: code.to_string(),
        name: name.to_string(),
        public_key: hex_encode(&public_key.0),
        signature: hex_encode(&signature.0),
    };
    let text = serde_json::to_string(&pair_frame)?;
    tx.send(Message::Text(text.into())).await?;

    let frame = next_frame(&mut rx).await?;
    let device_id = match frame {
        HandshakeFrame::Welcome { device_id } => device_id,
        HandshakeFrame::Reject { reason } => bail!("daemon rejected pairing: {reason}"),
        other => bail!("unexpected frame: {other:?}"),
    };
    let fingerprint = seen
        .lock()
        .unwrap()
        .clone()
        .unwrap_or_else(|| expected_fingerprint.unwrap_or_default().to_string());
    tx.send(Message::Close(None)).await.ok();
    Ok(PairOutcome {
        device_id,
        fingerprint,
    })
}

async fn next_frame<S>(rx: &mut S) -> Result<HandshakeFrame>
where
    S: StreamExt<Item = std::result::Result<Message, tokio_tungstenite::tungstenite::Error>>
        + Unpin,
{
    let msg = timeout(Duration::from_secs(10), rx.next())
        .await
        .map_err(|_| anyhow!("timed out waiting for server frame"))?
        .ok_or_else(|| anyhow!("server closed connection"))?
        .map_err(|e| anyhow!("ws error: {e}"))?;
    let text = match msg {
        Message::Text(t) => t.to_string(),
        other => bail!("unexpected frame: {other:?}"),
    };
    Ok(serde_json::from_str(&text)?)
}

fn hex_decode32(s: &str) -> Result<[u8; 32]> {
    if s.len() != 64 {
        bail!("nonce must be 64 hex chars, got {}", s.len());
    }
    let mut out = [0u8; 32];
    for (i, slot) in out.iter_mut().enumerate() {
        let hi = decode_nibble(s.as_bytes()[i * 2])?;
        let lo = decode_nibble(s.as_bytes()[i * 2 + 1])?;
        *slot = (hi << 4) | lo;
    }
    Ok(out)
}

fn decode_nibble(b: u8) -> Result<u8> {
    Ok(match b {
        b'0'..=b'9' => b - b'0',
        b'a'..=b'f' => b - b'a' + 10,
        b'A'..=b'F' => b - b'A' + 10,
        other => bail!("invalid hex character: {}", other as char),
    })
}

#[derive(Debug)]
struct PinVerifier {
    expected: Option<String>,
    seen: Arc<Mutex<Option<String>>>,
}

impl ServerCertVerifier for PinVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> std::result::Result<ServerCertVerified, TlsError> {
        let got = harness_transport::fingerprint(end_entity);
        *self.seen.lock().expect("mutex poisoned") = Some(got.clone());
        match &self.expected {
            Some(expected) if expected != &got => Err(TlsError::General(format!(
                "fingerprint mismatch: expected {expected} got {got}"
            ))),
            _ => Ok(ServerCertVerified::assertion()),
        }
    }
    fn verify_tls12_signature(
        &self,
        _: &[u8],
        _: &CertificateDer<'_>,
        _: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, TlsError> {
        Ok(HandshakeSignatureValid::assertion())
    }
    fn verify_tls13_signature(
        &self,
        _: &[u8],
        _: &CertificateDer<'_>,
        _: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, TlsError> {
        Ok(HandshakeSignatureValid::assertion())
    }
    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![
            SignatureScheme::ECDSA_NISTP256_SHA256,
            SignatureScheme::RSA_PSS_SHA256,
            SignatureScheme::RSA_PKCS1_SHA256,
            SignatureScheme::ED25519,
        ]
    }
}
