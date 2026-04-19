use std::collections::HashMap;
use std::net::SocketAddr;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallbackResult {
    Ok {
        code: String,
        state: String,
    },
    Err {
        error: String,
        description: Option<String>,
    },
}

pub async fn spawn_loopback() -> std::io::Result<(
    SocketAddr,
    tokio::sync::oneshot::Receiver<CallbackResult>,
    tokio::task::JoinHandle<()>,
)> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    let (tx, rx) = tokio::sync::oneshot::channel();
    let handle = tokio::spawn(async move {
        if let Ok((mut stream, _)) = listener.accept().await {
            let mut reader = BufReader::new(&mut stream);
            let mut request_line = String::new();
            reader.read_line(&mut request_line).await.ok();
            let mut header = String::new();
            while reader
                .read_line(&mut header)
                .await
                .map(|n| n > 2)
                .unwrap_or(false)
            {
                header.clear();
            }
            let result = parse_request_line(&request_line);
            let body = match &result {
                CallbackResult::Ok { .. } => ok_page().to_string(),
                CallbackResult::Err { error, .. } => err_page(error),
            };
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(resp.as_bytes()).await.ok();
            stream.shutdown().await.ok();
            tx.send(result).ok();
        }
    });
    Ok((addr, rx, handle))
}

fn parse_request_line(line: &str) -> CallbackResult {
    let path = line.split_whitespace().nth(1).unwrap_or("");
    let query = path.split_once('?').map_or("", |(_, q)| q);
    let params = parse_query(query);

    if let Some(error) = params.get("error") {
        return CallbackResult::Err {
            error: error.clone(),
            description: params.get("error_description").cloned(),
        };
    }
    let Some(code) = params.get("code").cloned() else {
        return CallbackResult::Err {
            error: "missing_code".into(),
            description: Some("no `code` in callback query".into()),
        };
    };
    CallbackResult::Ok {
        code,
        state: params.get("state").cloned().unwrap_or_default(),
    }
}

fn parse_query(q: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for pair in q.split('&') {
        let Some((k, v)) = pair.split_once('=') else {
            continue;
        };
        out.insert(percent_decode(k), percent_decode(v));
    }
    out
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hi = hex_val(bytes[i + 1]);
            let lo = hex_val(bytes[i + 2]);
            if let (Some(h), Some(l)) = (hi, lo) {
                out.push((h << 4) | l);
                i += 3;
                continue;
            }
        }
        if bytes[i] == b'+' {
            out.push(b' ');
        } else {
            out.push(bytes[i]);
        }
        i += 1;
    }
    String::from_utf8(out).unwrap_or_default()
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn ok_page() -> &'static str {
    r#"<!doctype html><html><head><meta charset="utf-8"><title>harness</title></head>
<body style="font-family: system-ui; max-width: 36rem; margin: 4rem auto; text-align: center;">
<h1>✓ You can close this tab.</h1>
<p>harness has your token now. Head back to the terminal.</p>
</body></html>"#
}

fn err_page(reason: &str) -> String {
    format!(
        r#"<!doctype html><html><head><meta charset="utf-8"><title>harness</title></head>
<body style="font-family: system-ui; max-width: 36rem; margin: 4rem auto; text-align: center;">
<h1>✗ Sign-in did not complete</h1>
<p><code>{reason}</code></p>
<p>Return to the terminal and try again.</p>
</body></html>"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ok_redirect() {
        let r = parse_request_line("GET /callback?code=abc123&state=xyz HTTP/1.1");
        assert_eq!(
            r,
            CallbackResult::Ok {
                code: "abc123".into(),
                state: "xyz".into(),
            }
        );
    }

    #[test]
    fn parses_error_redirect() {
        let r = parse_request_line(
            "GET /callback?error=access_denied&error_description=user%20said%20no HTTP/1.1",
        );
        match r {
            CallbackResult::Err { error, description } => {
                assert_eq!(error, "access_denied");
                assert_eq!(description.as_deref(), Some("user said no"));
            }
            CallbackResult::Ok { .. } => panic!("expected Err"),
        }
    }

    #[test]
    fn missing_code_is_error() {
        let r = parse_request_line("GET /callback?state=zzz HTTP/1.1");
        match r {
            CallbackResult::Err { error, .. } => assert_eq!(error, "missing_code"),
            CallbackResult::Ok { .. } => panic!("expected Err"),
        }
    }
}
