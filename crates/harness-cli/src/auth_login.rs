use anyhow::{Result, anyhow, bail};
use dialoguer::{Input, Password, Select};
use harness_auth::oauth::{
    CallbackResult, TokenBundle, authorize_url, begin_device_code, exchange_code, gen_pkce,
    poll_until_complete, spawn_loopback,
};
use serde_json::{Value, json};
use std::time::Duration;

use crate::daemon_rpc;
use crate::progress::Steps;

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
    let theme = crate::style::dialoguer_theme();
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
    let theme = crate::style::dialoguer_theme();
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
    let theme = crate::style::dialoguer_theme();
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
    let theme = crate::style::dialoguer_theme();
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
    let mut steps = Steps::new("ChatGPT sign-in (browser)");
    let s_listen = steps.add("listen on localhost:1455");
    let s_browser = steps.add("open browser");
    let s_callback = steps.add("wait for browser callback");
    let s_exchange = steps.add("exchange token");
    let s_store = steps.add("store credential");

    steps.start(s_listen);
    let pkce = gen_pkce();
    let (rx, _server) = match spawn_loopback().await {
        Ok(v) => v,
        Err(e) => {
            steps.fail(s_listen, &e.to_string());
            return Err(e.into());
        }
    };
    let url = authorize_url(&pkce);
    steps.ok(s_listen);

    steps.start(s_browser);
    match open::that(&url) {
        Ok(()) => steps.ok(s_browser),
        Err(e) => steps.fail(
            s_browser,
            &format!("could not launch ({e}) — paste the URL below manually"),
        ),
    }
    println!("    {url}");

    steps.start(s_callback);
    let cb = match rx.await {
        Ok(v) => v,
        Err(_) => {
            let msg = "loopback callback channel closed before redirect";
            steps.fail(s_callback, msg);
            bail!(msg);
        }
    };
    let (code, state) = match cb {
        CallbackResult::Ok { code, state } => (code, state),
        CallbackResult::Err { error, description } => {
            let msg = format!(
                "openai oauth declined: {error}{}",
                description.map(|d| format!(" — {d}")).unwrap_or_default()
            );
            steps.fail(s_callback, &msg);
            bail!(msg);
        }
    };
    if state != pkce.state {
        let msg = "state mismatch (possible csrf, refusing)";
        steps.fail(s_callback, msg);
        bail!("openai oauth: {msg}");
    }
    steps.ok(s_callback);

    steps.start(s_exchange);
    let bundle: TokenBundle = match exchange_code(&code, &pkce.verifier).await {
        Ok(b) => b,
        Err(e) => {
            steps.fail(s_exchange, &e.to_string());
            bail!("token exchange failed: {e}");
        }
    };
    steps.ok(s_exchange);

    save_oauth_credential(&mut steps, s_store, &bundle).await
}

async fn save_oauth_credential(
    steps: &mut Steps,
    s_store: usize,
    bundle: &TokenBundle,
) -> Result<()> {
    steps.start(s_store);
    let value =
        serde_json::to_string(bundle).map_err(|e| anyhow!("serialize token bundle: {e}"))?;
    let params = build_add_params("openai", "oauth", &value);
    if let Err(e) = daemon_rpc::call("v1.auth.credentials.add", Some(params)).await {
        steps.fail(s_store, &e.to_string());
        return Err(e);
    }
    steps.ok_message(s_store, "stored openai oauth credential");
    Ok(())
}

async fn codex_device_code_flow() -> Result<()> {
    let mut steps = Steps::new("ChatGPT sign-in (device code)");
    let s_request = steps.add("request device code");
    let s_poll = steps.add("wait for sign-in");
    let s_store = steps.add("store credential");

    steps.start(s_request);
    let device = match begin_device_code().await {
        Ok(d) => d,
        Err(e) => {
            steps.fail(s_request, &e.to_string());
            bail!("device code request failed: {e}");
        }
    };
    steps.ok(s_request);

    let minutes = Duration::from_secs(device.expires_in_secs).as_secs() / 60;
    println!("    open: {}", device.verification_url);
    println!("    code: {}", device.user_code);
    println!("    (expires in {minutes} minutes)");

    steps.start(s_poll);
    let bundle: TokenBundle = match poll_until_complete(&device).await {
        Ok(b) => b,
        Err(e) => {
            steps.fail(s_poll, &e.to_string());
            bail!("device-code poll failed: {e}");
        }
    };
    steps.ok(s_poll);

    save_oauth_credential(&mut steps, s_store, &bundle).await
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
