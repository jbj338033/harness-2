use crate::handshake::{
    AuthAttempt, AuthOutcome, Authenticator, HandshakeFrame, Nonce, hex_decode, hex_encode,
};
use crate::tls::TlsMaterials;
use crate::{Result, TransportError};
use axum::{
    Router as AxumRouter,
    extract::{
        State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    response::IntoResponse,
    routing::get,
};
use futures::{SinkExt, StreamExt};
use harness_lifecycle::Shutdown;
use harness_proto::{ErrorCode, ErrorObject, Id, Request, Response};
use harness_rpc::{Router, Sink};
use rand::RngCore;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

#[derive(Clone)]
pub struct WsConfig {
    pub addr: SocketAddr,
    pub tls: Option<TlsMaterials>,
    pub authenticator: Option<Arc<dyn Authenticator>>,
}

impl Default for WsConfig {
    fn default() -> Self {
        Self {
            addr: "0.0.0.0:8384".parse().expect("static address"),
            tls: None,
            authenticator: None,
        }
    }
}

impl std::fmt::Debug for WsConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WsConfig")
            .field("addr", &self.addr)
            .field("tls", &self.tls.as_ref().map(|t| &t.fingerprint_hex))
            .field("authenticator", &self.authenticator.is_some())
            .finish()
    }
}

#[derive(Clone)]
struct AppState {
    router: Arc<Router>,
    shutdown: Shutdown,
    authenticator: Option<Arc<dyn Authenticator>>,
}

pub async fn serve_ws(cfg: WsConfig, router: Router, shutdown: Shutdown) -> Result<()> {
    let app_state = AppState {
        router: Arc::new(router),
        shutdown: shutdown.clone(),
        authenticator: cfg.authenticator.clone(),
    };

    let app: AxumRouter = AxumRouter::new()
        .route("/", get(ws_upgrade))
        .with_state(app_state);

    match cfg.tls {
        Some(materials) => serve_tls(cfg.addr, materials, app, shutdown).await,
        None => serve_plain(cfg.addr, app, shutdown).await,
    }
}

async fn serve_plain(addr: SocketAddr, app: AxumRouter, shutdown: Shutdown) -> Result<()> {
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| TransportError::Bind(e.to_string()))?;
    info!(addr = %addr, "ws (plain) listening");
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            shutdown.cancelled().await;
        })
        .await
        .map_err(|e| TransportError::Ws(e.to_string()))
}

async fn serve_tls(
    addr: SocketAddr,
    materials: TlsMaterials,
    app: AxumRouter,
    shutdown: Shutdown,
) -> Result<()> {
    install_default_crypto_provider();
    let config = materials
        .server_config()
        .map_err(|e| TransportError::Ws(format!("tls config: {e}")))?;
    let rustls_config = axum_server::tls_rustls::RustlsConfig::from_config(Arc::new(config));

    info!(
        addr = %addr,
        fingerprint = %materials.fingerprint_hex,
        "wss (tls) listening"
    );

    let server = axum_server::bind_rustls(addr, rustls_config).serve(app.into_make_service());

    tokio::select! {
        res = server => res.map_err(|e| TransportError::Ws(e.to_string())),
        () = shutdown.cancelled() => Ok(()),
    }
}

fn install_default_crypto_provider() {
    rustls::crypto::ring::default_provider()
        .install_default()
        .ok();
}

async fn ws_upgrade(State(state): State<AppState>, ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_ws(socket, state))
}

async fn handle_ws(socket: WebSocket, state: AppState) {
    let (mut ws_tx, mut ws_rx) = socket.split();

    if let Some(auth) = state.authenticator.clone() {
        let nonce = fresh_nonce();
        let challenge = HandshakeFrame::Challenge {
            nonce: hex_encode(&nonce.value),
            issued_at: nonce.issued_at,
        };
        let challenge_text = match serde_json::to_string(&challenge) {
            Ok(s) => s,
            Err(e) => {
                warn!(error = %e, "failed to serialise challenge");
                return;
            }
        };
        if ws_tx
            .send(Message::Text(challenge_text.into()))
            .await
            .is_err()
        {
            return;
        }

        let Ok(Some(attempt)) = tokio::time::timeout(
            std::time::Duration::from_secs(30),
            next_text_frame(&mut ws_rx),
        )
        .await
        else {
            send_reject(&mut ws_tx, "handshake timeout").await.ok();
            return;
        };

        let parsed: HandshakeFrame = match serde_json::from_str(&attempt) {
            Ok(p) => p,
            Err(e) => {
                send_reject(&mut ws_tx, &format!("invalid handshake: {e}"))
                    .await
                    .ok();
                return;
            }
        };

        let auth_attempt = match parse_attempt(parsed) {
            Ok(a) => a,
            Err(reason) => {
                send_reject(&mut ws_tx, &reason).await.ok();
                return;
            }
        };

        match auth.verify(nonce, auth_attempt).await {
            AuthOutcome::Accepted { device_id } => {
                let welcome = HandshakeFrame::Welcome {
                    device_id: device_id.clone(),
                };
                let welcome_text = serde_json::to_string(&welcome).unwrap_or_default();
                if ws_tx
                    .send(Message::Text(welcome_text.into()))
                    .await
                    .is_err()
                {
                    return;
                }
                info!(device_id = %device_id, "ws client authenticated");
            }
            AuthOutcome::Rejected { reason } => {
                send_reject(&mut ws_tx, &reason).await.ok();
                return;
            }
        }
    }

    let (tx, mut rx) = mpsc::channel::<String>(64);
    let sink = Sink::new(tx);

    let writer_shutdown = state.shutdown.clone();
    let writer = tokio::spawn(async move {
        loop {
            tokio::select! {
                () = writer_shutdown.cancelled() => break,
                msg = rx.recv() => {
                    match msg {
                        None => break,
                        Some(text) => {
                            if ws_tx.send(Message::Text(text.into())).await.is_err() {
                                break;
                            }
                        }
                    }
                }
            }
        }
    });

    loop {
        tokio::select! {
            () = state.shutdown.cancelled() => break,
            frame = ws_rx.next() => {
                match frame {
                    Some(Err(e)) => {
                        debug!(error = %e, "ws read error");
                        break;
                    }
                    None | Some(Ok(Message::Close(_))) => break,
                    Some(Ok(Message::Ping(_) | Message::Pong(_) | Message::Binary(_))) => {}
                    Some(Ok(Message::Text(text))) => {
                        process_ws_text(&text, &state.router, sink.clone()).await;
                    }
                }
            }
        }
    }

    drop(sink);
    writer.await.ok();
}

fn fresh_nonce() -> Nonce {
    let mut value = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut value);
    let issued_at = i64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis(),
    )
    .unwrap_or(0);
    Nonce { value, issued_at }
}

async fn send_reject<S>(ws_tx: &mut S, reason: &str) -> std::result::Result<(), ()>
where
    S: SinkExt<Message> + Unpin,
{
    let reject = HandshakeFrame::Reject {
        reason: reason.into(),
    };
    let text = serde_json::to_string(&reject).unwrap_or_default();
    ws_tx.send(Message::Text(text.into())).await.ok();
    ws_tx.send(Message::Close(None)).await.ok();
    Ok(())
}

async fn next_text_frame<S>(ws_rx: &mut S) -> Option<String>
where
    S: StreamExt<Item = std::result::Result<Message, axum::Error>> + Unpin,
{
    while let Some(frame) = ws_rx.next().await {
        match frame {
            Ok(Message::Text(t)) => return Some(t.to_string()),
            Ok(Message::Close(_)) | Err(_) => return None,
            _ => {}
        }
    }
    None
}

fn parse_attempt(frame: HandshakeFrame) -> std::result::Result<AuthAttempt, String> {
    match frame {
        HandshakeFrame::Auth {
            public_key,
            signature,
        } => {
            let pk =
                hex_decode::<32>(&public_key).map_err(|e| format!("invalid public_key: {e}"))?;
            let sig =
                hex_decode::<64>(&signature).map_err(|e| format!("invalid signature: {e}"))?;
            Ok(AuthAttempt::Existing {
                public_key: pk,
                signature: sig,
            })
        }
        HandshakeFrame::Pair {
            code,
            name,
            public_key,
            signature,
        } => {
            let pk =
                hex_decode::<32>(&public_key).map_err(|e| format!("invalid public_key: {e}"))?;
            let sig =
                hex_decode::<64>(&signature).map_err(|e| format!("invalid signature: {e}"))?;
            Ok(AuthAttempt::Pairing {
                code,
                name,
                public_key: pk,
                signature: sig,
            })
        }
        HandshakeFrame::Challenge { .. }
        | HandshakeFrame::Welcome { .. }
        | HandshakeFrame::Reject { .. } => Err("expected auth or pair frame".into()),
    }
}

async fn process_ws_text(text: &str, router: &Router, sink: Sink) {
    let req: Request = match serde_json::from_str(text) {
        Ok(r) => r,
        Err(e) => {
            warn!(error = %e, "ws parse error");
            let resp = Response::err(
                Id::Null,
                ErrorObject::new(ErrorCode::ParseError, format!("parse error: {e}")),
            );
            if let Ok(s) = serde_json::to_string(&resp) {
                sink.send_raw(s).await.ok();
            }
            return;
        }
    };

    let resp = router.dispatch(req, sink.clone()).await;
    if let Ok(s) = serde_json::to_string(&resp) {
        sink.send_raw(s).await.ok();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tls::TlsMaterials;
    use futures::{SinkExt, StreamExt};
    use harness_rpc::{Router, handler};
    use serde_json::json;
    use tempfile::TempDir;
    use tokio_tungstenite::tungstenite::protocol::Message as TMessage;

    fn free_addr() -> SocketAddr {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap()
    }

    #[tokio::test]
    async fn ping_over_plain_ws() {
        let addr = free_addr();
        let router = Router::new().route(
            "ping",
            handler(|_p, _s| async move { Ok(json!({"pong": true})) }),
        );
        let shutdown = Shutdown::new();

        let srv_shutdown = shutdown.clone();
        let server = tokio::spawn(async move {
            serve_ws(
                WsConfig {
                    addr,
                    tls: None,
                    authenticator: None,
                },
                router,
                srv_shutdown,
            )
            .await
            .unwrap();
        });
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let url = format!("ws://{addr}");
        let (mut ws, _resp) = tokio_tungstenite::connect_async(&url).await.unwrap();

        let req = Request::new(Id::Number(1), "ping", None);
        ws.send(TMessage::Text(serde_json::to_string(&req).unwrap().into()))
            .await
            .unwrap();

        let msg = ws.next().await.unwrap().unwrap();
        let text = match msg {
            TMessage::Text(t) => t.to_string(),
            other => panic!("unexpected frame: {other:?}"),
        };
        let resp: Response = serde_json::from_str(&text).unwrap();
        assert_eq!(resp.id, Id::Number(1));

        shutdown.trigger();
        tokio::time::timeout(std::time::Duration::from_secs(2), server)
            .await
            .ok();
    }

    #[tokio::test]
    async fn ping_over_wss_with_pinned_fingerprint() {
        install_default_crypto_provider();
        let addr = free_addr();
        let tmp = TempDir::new().unwrap();
        let cert_path = tmp.path().join("tls/cert.pem");
        let key_path = tmp.path().join("tls/key.pem");
        let materials = TlsMaterials::load_or_create(&cert_path, &key_path, &[]).unwrap();
        let fingerprint = materials.fingerprint_hex.clone();

        let router = Router::new().route(
            "ping",
            handler(|_p, _s| async move { Ok(json!({"pong": true})) }),
        );
        let shutdown = Shutdown::new();
        let srv_shutdown = shutdown.clone();
        let server_materials = materials.clone();
        let server = tokio::spawn(async move {
            serve_ws(
                WsConfig {
                    addr,
                    tls: Some(server_materials),
                    authenticator: None,
                },
                router,
                srv_shutdown,
            )
            .await
            .unwrap();
        });
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        let tls = pinning_tls_connector(&fingerprint);
        let url = format!("wss://127.0.0.1:{}", addr.port());
        let request = url.into_client_request().unwrap();
        let (mut ws, _) = tokio_tungstenite::connect_async_tls_with_config(
            request,
            None,
            false,
            Some(tokio_tungstenite::Connector::Rustls(tls)),
        )
        .await
        .unwrap();

        let req = Request::new(Id::Number(2), "ping", None);
        ws.send(TMessage::Text(serde_json::to_string(&req).unwrap().into()))
            .await
            .unwrap();
        let msg = ws.next().await.unwrap().unwrap();
        let text = match msg {
            TMessage::Text(t) => t.to_string(),
            other => panic!("unexpected frame: {other:?}"),
        };
        let resp: Response = serde_json::from_str(&text).unwrap();
        assert_eq!(resp.id, Id::Number(2));

        shutdown.trigger();
        tokio::time::timeout(std::time::Duration::from_secs(2), server)
            .await
            .ok();
    }

    fn pinning_tls_connector(expected_fingerprint: &str) -> Arc<rustls::ClientConfig> {
        use rustls::client::danger::{
            HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier,
        };
        use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
        use rustls::{DigitallySignedStruct, Error as TlsError, SignatureScheme};

        #[derive(Debug)]
        struct Pin(String);
        impl ServerCertVerifier for Pin {
            fn verify_server_cert(
                &self,
                end_entity: &CertificateDer<'_>,
                _intermediates: &[CertificateDer<'_>],
                _server_name: &ServerName<'_>,
                _ocsp_response: &[u8],
                _now: UnixTime,
            ) -> std::result::Result<ServerCertVerified, TlsError> {
                let got = crate::tls::fingerprint(end_entity);
                if got == self.0 {
                    Ok(ServerCertVerified::assertion())
                } else {
                    Err(TlsError::General(format!(
                        "fingerprint mismatch: expected {} got {got}",
                        self.0
                    )))
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

        let config = rustls::ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(Pin(expected_fingerprint.into())))
            .with_no_client_auth();
        Arc::new(config)
    }

    use tokio_tungstenite::tungstenite::client::IntoClientRequest;

    struct AlwaysAccept;
    #[async_trait::async_trait]
    impl Authenticator for AlwaysAccept {
        async fn verify(&self, _nonce: Nonce, _attempt: AuthAttempt) -> AuthOutcome {
            AuthOutcome::Accepted {
                device_id: "dev-accept".into(),
            }
        }
    }

    struct AlwaysReject;
    #[async_trait::async_trait]
    impl Authenticator for AlwaysReject {
        async fn verify(&self, _nonce: Nonce, _attempt: AuthAttempt) -> AuthOutcome {
            AuthOutcome::Rejected {
                reason: "nope".into(),
            }
        }
    }

    #[tokio::test]
    async fn handshake_accept_allows_rpc() {
        let addr = free_addr();
        let router = Router::new().route(
            "ping",
            handler(|_p, _s| async move { Ok(json!({"pong": true})) }),
        );
        let shutdown = Shutdown::new();
        let srv_shutdown = shutdown.clone();
        let server = tokio::spawn(async move {
            serve_ws(
                WsConfig {
                    addr,
                    tls: None,
                    authenticator: Some(Arc::new(AlwaysAccept)),
                },
                router,
                srv_shutdown,
            )
            .await
            .unwrap();
        });
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let url = format!("ws://{addr}");
        let (mut ws, _) = tokio_tungstenite::connect_async(&url).await.unwrap();

        let chall = ws.next().await.unwrap().unwrap();
        let chall_text = match chall {
            TMessage::Text(t) => t.to_string(),
            other => panic!("unexpected frame: {other:?}"),
        };
        let frame: HandshakeFrame = serde_json::from_str(&chall_text).unwrap();
        assert!(matches!(frame, HandshakeFrame::Challenge { .. }));

        let auth = HandshakeFrame::Auth {
            public_key: "00".repeat(32),
            signature: "00".repeat(64),
        };
        ws.send(TMessage::Text(serde_json::to_string(&auth).unwrap().into()))
            .await
            .unwrap();

        let welcome = ws.next().await.unwrap().unwrap();
        let welcome_text = match welcome {
            TMessage::Text(t) => t.to_string(),
            other => panic!("unexpected frame: {other:?}"),
        };
        let frame: HandshakeFrame = serde_json::from_str(&welcome_text).unwrap();
        assert!(matches!(frame, HandshakeFrame::Welcome { .. }));

        let req = Request::new(Id::Number(1), "ping", None);
        ws.send(TMessage::Text(serde_json::to_string(&req).unwrap().into()))
            .await
            .unwrap();
        let msg = ws.next().await.unwrap().unwrap();
        let text = match msg {
            TMessage::Text(t) => t.to_string(),
            other => panic!("unexpected frame: {other:?}"),
        };
        let resp: Response = serde_json::from_str(&text).unwrap();
        assert_eq!(resp.id, Id::Number(1));

        shutdown.trigger();
        tokio::time::timeout(std::time::Duration::from_secs(2), server)
            .await
            .ok();
    }

    #[tokio::test]
    async fn handshake_reject_closes_connection() {
        let addr = free_addr();
        let router = Router::new();
        let shutdown = Shutdown::new();
        let srv_shutdown = shutdown.clone();
        let server = tokio::spawn(async move {
            serve_ws(
                WsConfig {
                    addr,
                    tls: None,
                    authenticator: Some(Arc::new(AlwaysReject)),
                },
                router,
                srv_shutdown,
            )
            .await
            .unwrap();
        });
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let url = format!("ws://{addr}");
        let (mut ws, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
        ws.next().await;
        let auth = HandshakeFrame::Auth {
            public_key: "00".repeat(32),
            signature: "00".repeat(64),
        };
        ws.send(TMessage::Text(serde_json::to_string(&auth).unwrap().into()))
            .await
            .unwrap();
        let msg = ws.next().await.unwrap().unwrap();
        let text = match msg {
            TMessage::Text(t) => t.to_string(),
            other => panic!("unexpected frame: {other:?}"),
        };
        let frame: HandshakeFrame = serde_json::from_str(&text).unwrap();
        assert!(matches!(frame, HandshakeFrame::Reject { .. }));

        shutdown.trigger();
        tokio::time::timeout(std::time::Duration::from_secs(2), server)
            .await
            .ok();
    }
}
