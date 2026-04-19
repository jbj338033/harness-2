use super::TokenBundle;
use super::openai::{CLIENT_ID, DEVICE_CODE_URL, OAuthError, SCOPES, TOKEN_URL};
use serde::Deserialize;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Deserialize)]
pub struct DeviceCode {
    pub user_code: String,
    pub verification_uri: String,
    #[serde(default)]
    pub verification_uri_complete: Option<String>,
    pub device_code: String,
    #[serde(default = "default_interval")]
    pub interval: u64,
    pub expires_in: u64,
}

fn default_interval() -> u64 {
    5
}

pub async fn begin() -> Result<DeviceCode, OAuthError> {
    let client = reqwest::Client::builder().build()?;
    let resp = client
        .post(DEVICE_CODE_URL)
        .form(&[("client_id", CLIENT_ID), ("scope", SCOPES)])
        .send()
        .await?;
    let status = resp.status();
    let text = resp.text().await?;
    if !status.is_success() {
        return Err(OAuthError::TokenEndpoint {
            status: status.as_u16(),
            code: "device_code_begin".into(),
            description: Some(text),
        });
    }
    Ok(serde_json::from_str(&text)?)
}

pub async fn poll_until_complete(code: &DeviceCode) -> Result<TokenBundle, OAuthError> {
    let deadline = std::time::Instant::now() + Duration::from_secs(code.expires_in);
    let mut interval = Duration::from_secs(code.interval);
    let client = reqwest::Client::builder().build()?;

    loop {
        if std::time::Instant::now() >= deadline {
            return Err(OAuthError::TokenEndpoint {
                status: 0,
                code: "expired_token".into(),
                description: Some(
                    "device authorization expired — run `harness auth login openai --device-auth` again"
                        .into(),
                ),
            });
        }
        tokio::time::sleep(interval).await;

        let resp = client
            .post(TOKEN_URL)
            .form(&[
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ("device_code", code.device_code.as_str()),
                ("client_id", CLIENT_ID),
            ])
            .send()
            .await?;
        let status = resp.status();
        let text = resp.text().await?;

        if status.is_success() {
            let body: TokenResponse = serde_json::from_str(&text)?;
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
                .unwrap_or_default();
            return Ok(TokenBundle {
                access_token: body.access_token,
                refresh_token: body.refresh_token.unwrap_or_default(),
                expires_at: now + i64::from(body.expires_in),
                id_token: body.id_token,
            });
        }

        let err: TokenError = serde_json::from_str(&text).unwrap_or(TokenError {
            error: "unknown".into(),
            error_description: Some(text.clone()),
        });
        match err.error.as_str() {
            "authorization_pending" => {}
            "slow_down" => interval += Duration::from_secs(5),
            _ => {
                return Err(OAuthError::TokenEndpoint {
                    status: status.as_u16(),
                    code: err.error,
                    description: err.error_description,
                });
            }
        }
    }
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
struct TokenError {
    error: String,
    #[serde(default)]
    error_description: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_code_deserializes_with_defaults() {
        let src = r#"{
            "user_code": "ABCD-EFGH",
            "verification_uri": "https://auth.openai.com/activate",
            "device_code": "dc_abc",
            "expires_in": 900
        }"#;
        let dc: DeviceCode = serde_json::from_str(src).unwrap();
        assert_eq!(dc.user_code, "ABCD-EFGH");
        assert_eq!(dc.interval, 5);
        assert_eq!(dc.expires_in, 900);
    }
}
