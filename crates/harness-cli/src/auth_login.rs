use anyhow::{Result, anyhow, bail};
use dialoguer::theme::ColorfulTheme;
use dialoguer::{Input, Password, Select};
use harness_auth::oauth::{
    CallbackResult, TokenBundle, authorize_url, begin_device_code, exchange_code, gen_pkce,
    poll_until_complete, spawn_loopback,
};
use serde_json::{Value, json};

use crate::daemon_rpc;

const PROVIDERS: &[&str] = &[
    "Anthropic   API key (Claude)",
    "OpenAI      API key / Codex OAuth",
    "Gemini      API key (Google)",
    "Ollama      local endpoint",
    "Skip for now",
];

fn provider_default_index(preselected: Option<&str>) -> usize {
    match preselected.map(str::to_ascii_lowercase).as_deref() {
        Some("openai" | "chatgpt" | "codex") => 1,
        Some("google" | "gemini") => 2,
        Some("ollama" | "local") => 3,
        _ => 0,
    }
}

pub async fn run(preselected: Option<&str>) -> Result<()> {
    let theme = ColorfulTheme::default();
    let pick = Select::with_theme(&theme)
        .with_prompt("where do your models come from?")
        .items(PROVIDERS)
        .default(provider_default_index(preselected))
        .interact_opt()?;
    let Some(idx) = pick else {
        return Ok(());
    };
    match idx {
        0 => api_key_flow("anthropic", "Anthropic").await,
        1 => openai_flow().await,
        2 => api_key_flow("google", "Gemini").await,
        3 => ollama_flow().await,
        _ => Ok(()),
    }
}

async fn openai_flow() -> Result<()> {
    let theme = ColorfulTheme::default();
    let pick = Select::with_theme(&theme)
        .with_prompt("OpenAI sign-in method")
        .items([
            "API key (sk-...)",
            "Codex OAuth (ChatGPT plan, opens browser)",
            "Codex OAuth (device code, headless / SSH)",
        ])
        .default(0)
        .interact_opt()?;
    match pick {
        Some(0) => api_key_flow("openai", "OpenAI").await,
        Some(1) => codex_oauth_flow().await,
        Some(2) => codex_device_code_flow().await,
        _ => Ok(()),
    }
}

async fn api_key_flow(provider: &str, label: &str) -> Result<()> {
    let theme = ColorfulTheme::default();
    let key: String = Password::with_theme(&theme)
        .with_prompt(format!("{label} API key"))
        .allow_empty_password(false)
        .interact()?;
    let params = build_add_params(provider, "api_key", &key);
    daemon_rpc::call("v1.auth.credentials.add", Some(params)).await?;
    println!("✓ saved {provider} credential");
    Ok(())
}

async fn ollama_flow() -> Result<()> {
    let theme = ColorfulTheme::default();
    let url: String = Input::with_theme(&theme)
        .with_prompt("Ollama endpoint")
        .with_initial_text("http://localhost:11434")
        .interact_text()?;
    daemon_rpc::call(
        "v1.config.set",
        Some(json!({ "key": "ollama.endpoint", "value": url.trim() })),
    )
    .await?;
    println!("✓ ollama endpoint set to {}", url.trim());
    Ok(())
}

async fn codex_oauth_flow() -> Result<()> {
    let pkce = gen_pkce();
    let (addr, rx, _server) = spawn_loopback().await?;
    let redirect = format!("http://127.0.0.1:{}/callback", addr.port());
    let url = authorize_url(&pkce, &redirect);

    println!("opening browser for ChatGPT sign-in:");
    println!("  {url}");
    if let Err(e) = open::that(&url) {
        eprintln!("could not launch browser ({e}) — paste the URL above manually");
    }
    println!("waiting for the browser callback...");

    let cb = rx
        .await
        .map_err(|_| anyhow!("loopback callback channel closed before redirect"))?;
    let (code, state) = match cb {
        CallbackResult::Ok { code, state } => (code, state),
        CallbackResult::Err { error, description } => {
            bail!(
                "openai oauth declined: {error}{}",
                description.map(|d| format!(" — {d}")).unwrap_or_default()
            );
        }
    };
    if state != pkce.state {
        bail!("openai oauth: state mismatch (possible csrf, refusing)");
    }
    let bundle: TokenBundle = exchange_code(&code, &pkce.verifier, &redirect)
        .await
        .map_err(|e| anyhow!("token exchange failed: {e}"))?;

    let value =
        serde_json::to_string(&bundle).map_err(|e| anyhow!("serialize token bundle: {e}"))?;
    let params = build_add_params("openai", "oauth", &value);
    daemon_rpc::call("v1.auth.credentials.add", Some(params)).await?;
    println!("✓ saved openai oauth credential");
    Ok(())
}

async fn codex_device_code_flow() -> Result<()> {
    let device = begin_device_code()
        .await
        .map_err(|e| anyhow!("device code request failed: {e}"))?;
    println!("\nopen this URL on any device with a browser:");
    println!(
        "  {}",
        device
            .verification_uri_complete
            .as_deref()
            .unwrap_or(device.verification_uri.as_str())
    );
    println!("then enter this code if asked: {}\n", device.user_code);
    println!(
        "(this code expires in {} minutes; harness will keep polling)",
        device.expires_in / 60
    );

    let bundle: TokenBundle = poll_until_complete(&device)
        .await
        .map_err(|e| anyhow!("device-code poll failed: {e}"))?;
    let value =
        serde_json::to_string(&bundle).map_err(|e| anyhow!("serialize token bundle: {e}"))?;
    let params = build_add_params("openai", "oauth", &value);
    daemon_rpc::call("v1.auth.credentials.add", Some(params)).await?;
    println!("✓ saved openai oauth credential");
    Ok(())
}

#[must_use]
pub fn build_add_params(provider: &str, kind: &str, value: &str) -> Value {
    json!({
        "provider": provider,
        "kind": kind,
        "value": value,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_params_anthropic_api_key() {
        let p = build_add_params("anthropic", "api_key", "sk-ant-test");
        assert_eq!(p["provider"], "anthropic");
        assert_eq!(p["kind"], "api_key");
        assert_eq!(p["value"], "sk-ant-test");
    }

    #[test]
    fn add_params_openai_oauth_carries_json_blob() {
        let bundle = r#"{"access_token":"a","refresh_token":"r","expires_at":1}"#;
        let p = build_add_params("openai", "oauth", bundle);
        assert_eq!(p["provider"], "openai");
        assert_eq!(p["kind"], "oauth");
        assert_eq!(p["value"], bundle);
    }

    #[test]
    fn provider_default_resolves_aliases() {
        assert_eq!(provider_default_index(Some("claude")), 0);
        assert_eq!(provider_default_index(Some("OpenAI")), 1);
        assert_eq!(provider_default_index(Some("codex")), 1);
        assert_eq!(provider_default_index(Some("gemini")), 2);
        assert_eq!(provider_default_index(Some("ollama")), 3);
        assert_eq!(provider_default_index(Some("zzz")), 0);
        assert_eq!(provider_default_index(None), 0);
    }
}
