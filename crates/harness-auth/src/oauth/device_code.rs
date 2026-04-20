use super::TokenBundle;
use super::openai::{CLIENT_ID, OAuthError, exchange_code_with_redirect};
use serde::Deserialize;
use std::time::{Duration, Instant};

const USERCODE_URL: &str = "https://auth.openai.com/api/accounts/deviceauth/usercode";
const POLL_URL: &str = "https://auth.openai.com/api/accounts/deviceauth/token";
const VERIFICATION_URL: &str = "https://auth.openai.com/codex/device";
const DEVICEAUTH_REDIRECT_URI: &str = "https://auth.openai.com/deviceauth/callback";

const MAX_WAIT: Duration = Duration::from_secs(15 * 60);

#[derive(Debug, Clone)]
pub struct DeviceCode {
    pub user_code: String,
    pub verification_url: String,
    pub expires_in_secs: u64,
    device_auth_id: String,
    interval: u64,
}

#[derive(Deserialize)]
struct UserCodeResp {
    device_auth_id: String,
    #[serde(alias = "user_code", alias = "usercode")]
    user_code: String,
    #[serde(default = "default_interval", deserialize_with = "deserialize_u64")]
    interval: u64,
}

fn default_interval() -> u64 {
    5
}

fn deserialize_u64<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u64, D::Error> {
    use serde::de::Error;
    let s = String::deserialize(d)?;
    s.trim().parse::<u64>().map_err(D::Error::custom)
}

#[derive(Deserialize)]
struct PollSuccess {
    authorization_code: String,
    code_verifier: String,
}

pub async fn begin() -> Result<DeviceCode, OAuthError> {
    let client = reqwest::Client::builder().build()?;
    let body = serde_json::json!({ "client_id": CLIENT_ID });
    let resp = client
        .post(USERCODE_URL)
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(&body)?)
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
    let uc: UserCodeResp = serde_json::from_str(&text)?;
    Ok(DeviceCode {
        user_code: uc.user_code,
        device_auth_id: uc.device_auth_id,
        verification_url: VERIFICATION_URL.to_string(),
        expires_in_secs: MAX_WAIT.as_secs(),
        interval: uc.interval.max(1),
    })
}

pub async fn poll_until_complete(code: &DeviceCode) -> Result<TokenBundle, OAuthError> {
    let client = reqwest::Client::builder().build()?;
    let issued = poll_for_authorization(&client, code).await?;
    exchange_code_with_redirect(
        &issued.authorization_code,
        &issued.code_verifier,
        DEVICEAUTH_REDIRECT_URI,
    )
    .await
}

async fn poll_for_authorization(
    client: &reqwest::Client,
    code: &DeviceCode,
) -> Result<PollSuccess, OAuthError> {
    let body = serde_json::json!({
        "device_auth_id": code.device_auth_id,
        "user_code": code.user_code,
    });
    let body = serde_json::to_string(&body)?;
    let mut interval = Duration::from_secs(code.interval);
    let start = Instant::now();

    loop {
        if start.elapsed() >= MAX_WAIT {
            return Err(OAuthError::TokenEndpoint {
                status: 0,
                code: "expired_token".into(),
                description: Some("device authorization timed out after 15 minutes".into()),
            });
        }
        let resp = client
            .post(POLL_URL)
            .header("Content-Type", "application/json")
            .body(body.clone())
            .send()
            .await?;
        let status = resp.status();
        if status.is_success() {
            let text = resp.text().await?;
            return Ok(serde_json::from_str(&text)?);
        }
        if status == reqwest::StatusCode::FORBIDDEN || status == reqwest::StatusCode::NOT_FOUND {
            let sleep_for = interval.min(MAX_WAIT.saturating_sub(start.elapsed()));
            tokio::time::sleep(sleep_for).await;
            interval = (interval + Duration::from_millis(250)).min(Duration::from_secs(15));
            continue;
        }
        let text = resp.text().await?;
        return Err(OAuthError::TokenEndpoint {
            status: status.as_u16(),
            code: "device_code_poll".into(),
            description: Some(text),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_code_resp_deserializes_with_int_interval() {
        let raw = r#"{ "device_auth_id": "did", "user_code": "ABCD-1234", "interval": "5" }"#;
        let r: UserCodeResp = serde_json::from_str(raw).unwrap();
        assert_eq!(r.user_code, "ABCD-1234");
        assert_eq!(r.interval, 5);
    }

    #[test]
    fn user_code_resp_alias_usercode() {
        let raw = r#"{ "device_auth_id": "did", "usercode": "X-Y-Z" }"#;
        let r: UserCodeResp = serde_json::from_str(raw).unwrap();
        assert_eq!(r.user_code, "X-Y-Z");
        assert_eq!(r.interval, 5);
    }
}
