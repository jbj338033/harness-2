use super::pkce::Pkce;
use serde::{Deserialize, Serialize};

pub const CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";

pub const AUTHORIZE_URL: &str = "https://auth.openai.com/oauth/authorize";

pub const TOKEN_URL: &str = "https://auth.openai.com/oauth/token";

pub const DEVICE_CODE_URL: &str = "https://auth.openai.com/oauth/device/code";

pub const REVOKE_URL: &str = "https://auth.openai.com/oauth/revoke";

pub const CHATGPT_API_BASE: &str = "https://chatgpt.com/backend-api";

pub const SCOPES: &str = "openid profile email offline_access";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TokenBundle {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id_token: Option<String>,
}

impl TokenBundle {
    #[must_use]
    pub fn is_stale(&self, now_unix_s: i64, skew_s: i64) -> bool {
        self.expires_at <= now_unix_s + skew_s
    }
}

#[must_use]
pub fn authorize_url(pkce: &Pkce, redirect_uri: &str) -> String {
    let q = [
        ("response_type", "code"),
        ("client_id", CLIENT_ID),
        ("redirect_uri", redirect_uri),
        ("code_challenge", pkce.challenge.as_str()),
        ("code_challenge_method", "S256"),
        ("state", pkce.state.as_str()),
        ("scope", SCOPES),
    ];
    let mut url = AUTHORIZE_URL.to_string();
    url.push('?');
    for (i, (k, v)) in q.iter().enumerate() {
        if i > 0 {
            url.push('&');
        }
        url.push_str(k);
        url.push('=');
        url.push_str(&urlencode(v));
    }
    url
}

pub async fn exchange_code(
    code: &str,
    pkce_verifier: &str,
    redirect_uri: &str,
) -> Result<TokenBundle, OAuthError> {
    let params = [
        ("grant_type", "authorization_code"),
        ("code", code),
        ("code_verifier", pkce_verifier),
        ("client_id", CLIENT_ID),
        ("redirect_uri", redirect_uri),
    ];
    token_post(&params).await
}

pub async fn refresh_access_token(refresh_token: &str) -> Result<TokenBundle, OAuthError> {
    let params = [
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh_token),
        ("client_id", CLIENT_ID),
    ];
    token_post(&params).await
}

pub async fn revoke(token: &str) -> Result<(), reqwest::Error> {
    let client = reqwest::Client::builder().build()?;
    client
        .post(REVOKE_URL)
        .form(&[
            ("token", token),
            ("client_id", CLIENT_ID),
            ("token_type_hint", "refresh_token"),
        ])
        .send()
        .await?;
    Ok(())
}

async fn token_post(params: &[(&str, &str)]) -> Result<TokenBundle, OAuthError> {
    let client = reqwest::Client::builder().build()?;
    let resp = client.post(TOKEN_URL).form(params).send().await?;
    let status = resp.status();
    let text = resp.text().await?;
    if !status.is_success() {
        let err: TokenErrorBody = serde_json::from_str(&text).unwrap_or(TokenErrorBody {
            error: "unknown".into(),
            error_description: Some(text.clone()),
        });
        return Err(OAuthError::TokenEndpoint {
            status: status.as_u16(),
            code: err.error,
            description: err.error_description,
        });
    }
    let body: TokenResponse = serde_json::from_str(&text)?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
        .unwrap_or_default();
    Ok(TokenBundle {
        access_token: body.access_token,
        refresh_token: body.refresh_token.unwrap_or_default(),
        expires_at: now + i64::from(body.expires_in),
        id_token: body.id_token,
    })
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    expires_in: u32,
    #[serde(default)]
    id_token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TokenErrorBody {
    error: String,
    #[serde(default)]
    error_description: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum OAuthError {
    #[error(transparent)]
    Transport(#[from] reqwest::Error),
    #[error("bad token response: {0}")]
    BadResponse(#[from] serde_json::Error),
    #[error("oauth {code} (status {status}): {}", description.clone().unwrap_or_default())]
    TokenEndpoint {
        status: u16,
        code: String,
        description: Option<String>,
    },
}

fn urlencode(s: &str) -> String {
    use std::fmt::Write;
    const UNRESERVED: &[u8] = b"-_.~";
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || UNRESERVED.contains(&b) {
            out.push(b as char);
        } else {
            write!(out, "%{b:02X}").unwrap();
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authorize_url_contains_expected_params() {
        let pkce = Pkce {
            verifier: "v".into(),
            challenge: "c".into(),
            state: "s".into(),
        };
        let u = authorize_url(&pkce, "http://localhost:12345/callback");
        assert!(u.starts_with(AUTHORIZE_URL));
        assert!(u.contains("client_id=app_EMoamEEZ73f0CkXaXp7hrann"));
        assert!(u.contains("code_challenge=c"));
        assert!(u.contains("code_challenge_method=S256"));
        assert!(u.contains("state=s"));
        assert!(u.contains("response_type=code"));
        assert!(u.contains("redirect_uri=http%3A%2F%2Flocalhost%3A12345%2Fcallback"));
    }

    #[test]
    fn is_stale_catches_expiry_window() {
        let bundle = TokenBundle {
            access_token: "a".into(),
            refresh_token: "r".into(),
            expires_at: 1_000,
            id_token: None,
        };
        assert!(bundle.is_stale(940, 60));
        assert!(!bundle.is_stale(939, 60));
    }
}
