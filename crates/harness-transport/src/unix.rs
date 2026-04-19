use crate::{Result, TransportError};
use harness_lifecycle::Shutdown;
use harness_proto::Request;
use harness_rpc::{Router, Sink};
use std::path::Path;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

pub async fn serve_unix(path: impl AsRef<Path>, router: Router, shutdown: Shutdown) -> Result<()> {
    let path = path.as_ref();
    if path.exists() {
        std::fs::remove_file(path).ok();
    }

    let listener = UnixListener::bind(path).map_err(|e| TransportError::Bind(e.to_string()))?;
    info!(path = %path.display(), "unix socket listening");

    let router = Arc::new(router);

    loop {
        tokio::select! {
            () = shutdown.cancelled() => {
                info!("unix socket shutting down");
                break;
            }
            accept = listener.accept() => {
                match accept {
                    Ok((stream, _addr)) => {
                        let r = router.clone();
                        let s = shutdown.clone();
                        tokio::spawn(async move {
                            if let Err(e) = handle_client(stream, r, s).await {
                                warn!(error = %e, "unix client error");
                            }
                        });
                    }
                    Err(e) => error!(error = %e, "unix accept failed"),
                }
            }
        }
    }

    std::fs::remove_file(path).ok();
    Ok(())
}

async fn handle_client(stream: UnixStream, router: Arc<Router>, shutdown: Shutdown) -> Result<()> {
    let (read_half, mut write_half) = stream.into_split();
    let mut reader = BufReader::new(read_half);

    let (tx, mut rx) = mpsc::channel::<String>(64);
    let writer_shutdown = shutdown.clone();
    let writer = tokio::spawn(async move {
        loop {
            tokio::select! {
                () = writer_shutdown.cancelled() => break,
                msg = rx.recv() => {
                    match msg {
                        None => break,
                        Some(line) => {
                            if write_half.write_all(line.as_bytes()).await.is_err()
                                || write_half.write_all(b"\n").await.is_err()
                            {
                                break;
                            }
                        }
                    }
                }
            }
        }
    });
    let sink = Sink::new(tx);

    let mut line = String::new();
    loop {
        line.clear();
        tokio::select! {
            () = shutdown.cancelled() => break,
            n = reader.read_line(&mut line) => {
                match n {
                    Ok(0) => break,
                    Ok(_) => {
                        let trimmed = line.trim();
                        if trimmed.is_empty() {
                            continue;
                        }
                        process_line(trimmed, &router, sink.clone()).await;
                    }
                    Err(e) => {
                        debug!(error = %e, "unix read error; closing");
                        break;
                    }
                }
            }
        }
    }

    drop(sink);
    writer.await.ok();
    Ok(())
}

async fn process_line(line: &str, router: &Router, sink: Sink) {
    let req: Request = match serde_json::from_str(line) {
        Ok(r) => r,
        Err(e) => {
            let err_resp = harness_proto::Response::err(
                harness_proto::Id::Null,
                harness_proto::ErrorObject::new(
                    harness_proto::ErrorCode::ParseError,
                    format!("parse error: {e}"),
                ),
            );
            if let Ok(s) = serde_json::to_string(&err_resp) {
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
    use harness_proto::{Id, Response, ResponsePayload};
    use harness_rpc::{Router, handler};
    use serde_json::json;
    use tempfile::TempDir;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt};

    #[tokio::test]
    async fn ping_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let sock = tmp.path().join("test.sock");
        let router = Router::new().route(
            "ping",
            handler(|_p, _s| async move { Ok(json!({"pong": true})) }),
        );

        let shutdown = Shutdown::new();
        let shutdown_server = shutdown.clone();
        let sock_server = sock.clone();
        let server = tokio::spawn(async move {
            serve_unix(sock_server, router, shutdown_server)
                .await
                .unwrap();
        });

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let mut client = tokio::net::UnixStream::connect(&sock).await.unwrap();
        let req = harness_proto::Request::new(Id::Number(1), "ping", None);
        let line = serde_json::to_string(&req).unwrap();
        client.write_all(line.as_bytes()).await.unwrap();
        client.write_all(b"\n").await.unwrap();

        let (r, _) = client.split();
        let mut reader = tokio::io::BufReader::new(r);
        let mut response_line = String::new();
        reader.read_line(&mut response_line).await.unwrap();

        let resp: Response = serde_json::from_str(&response_line).unwrap();
        assert_eq!(resp.id, Id::Number(1));
        match resp.payload {
            ResponsePayload::Result(v) => assert_eq!(v, json!({"pong": true})),
            ResponsePayload::Error(_) => panic!("expected result"),
        }

        shutdown.trigger();
        tokio::time::timeout(std::time::Duration::from_secs(1), server)
            .await
            .ok();
    }

    #[tokio::test]
    async fn parse_error_returns_error_response() {
        let tmp = TempDir::new().unwrap();
        let sock = tmp.path().join("test.sock");
        let router = Router::new();

        let shutdown = Shutdown::new();
        let shutdown_server = shutdown.clone();
        let sock_server = sock.clone();
        let server = tokio::spawn(async move {
            serve_unix(sock_server, router, shutdown_server)
                .await
                .unwrap();
        });
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let mut client = tokio::net::UnixStream::connect(&sock).await.unwrap();
        client.write_all(b"not valid json\n").await.unwrap();

        let (r, _) = client.split();
        let mut reader = tokio::io::BufReader::new(r);
        let mut line = String::new();
        reader.read_line(&mut line).await.unwrap();
        let resp: Response = serde_json::from_str(&line).unwrap();
        match resp.payload {
            ResponsePayload::Error(e) => {
                assert_eq!(e.code, harness_proto::ErrorCode::ParseError as i32);
            }
            ResponsePayload::Result(_) => panic!("expected error"),
        }

        shutdown.trigger();
        tokio::time::timeout(std::time::Duration::from_secs(1), server)
            .await
            .ok();
    }

    #[tokio::test]
    async fn shutdown_unblocks_accept() {
        let tmp = TempDir::new().unwrap();
        let sock = tmp.path().join("test.sock");
        let router = Router::new();

        let shutdown = Shutdown::new();
        let shutdown_server = shutdown.clone();
        let sock_server = sock.clone();
        let server = tokio::spawn(async move {
            serve_unix(sock_server, router, shutdown_server)
                .await
                .unwrap();
        });
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        shutdown.trigger();
        let result = tokio::time::timeout(std::time::Duration::from_secs(1), server)
            .await
            .expect("server did not shut down in time");
        result.unwrap();

        assert!(!sock.exists());
    }
}
