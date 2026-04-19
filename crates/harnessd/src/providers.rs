use harness_llm::{Provider, ProviderPool, ProviderSlot};
use harness_llm_anthropic::AnthropicProvider;
use harness_llm_google::GoogleProvider;
use harness_llm_ollama::OllamaProvider;
use harness_llm_openai::OpenAiProvider;
use harness_storage::{WriterHandle, credentials};
use std::collections::BTreeSet;
use std::sync::Arc;
use tracing::{info, warn};

pub const DEFAULT_MODEL_BY_PROVIDER: &[(&str, &str)] = &[
    ("anthropic", "claude-sonnet-4-6"),
    ("openai", "gpt-5.4"),
    ("google", "gemini-3.1-pro"),
];

pub fn available_providers(
    conn: &rusqlite::Connection,
) -> harness_storage::Result<BTreeSet<String>> {
    Ok(credentials::list(conn)?
        .into_iter()
        .map(|c| c.provider)
        .collect())
}

pub fn suggest_default_model(
    conn: &rusqlite::Connection,
) -> harness_storage::Result<Option<String>> {
    let available = available_providers(conn)?;
    Ok(DEFAULT_MODEL_BY_PROVIDER
        .iter()
        .find(|(p, _)| available.contains(*p))
        .map(|(_, m)| (*m).to_string()))
}

pub fn build_pool(
    conn: &rusqlite::Connection,
    writer: &WriterHandle,
) -> harness_storage::Result<ProviderPool> {
    let creds = credentials::list(conn)?;
    let mut slots: Vec<ProviderSlot> = Vec::new();

    for c in creds {
        let label = c.label.clone().unwrap_or_else(|| c.id.clone());
        let provider: Arc<dyn Provider> = match (c.provider.as_str(), c.kind.as_str()) {
            ("anthropic", _) => Arc::new(AnthropicProvider::new(
                format!("anthropic:{label}"),
                c.value,
            )),
            ("openai", "oauth") => {
                match serde_json::from_str::<harness_auth::oauth::TokenBundle>(&c.value) {
                    Ok(bundle) => Arc::new(OpenAiProvider::new_oauth(
                        format!("openai:{label}(oauth)"),
                        c.id.clone(),
                        bundle,
                        writer.clone(),
                    )),
                    Err(e) => {
                        warn!(
                            credential_id = %c.id,
                            %e,
                            "openai oauth credential: malformed value, skipping"
                        );
                        continue;
                    }
                }
            }
            ("openai", _) => Arc::new(OpenAiProvider::new(format!("openai:{label}"), c.value)),
            ("google", _) => Arc::new(GoogleProvider::new(format!("google:{label}"), c.value)),
            (other, _) => {
                warn!(
                    provider = other,
                    "unknown provider family in credentials; skipping"
                );
                continue;
            }
        };
        slots.push(ProviderSlot::new(provider));
    }

    slots.push(ProviderSlot::new(Arc::new(OllamaProvider::new(
        "ollama:local",
    ))));

    info!(slot_count = slots.len(), "built provider pool");
    Ok(ProviderPool::new(slots))
}

#[cfg(test)]
mod tests {
    use super::*;
    use harness_storage::{Database, Writer, credentials as creds_store};
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn pool_has_ollama_even_without_credentials() {
        let f = NamedTempFile::new().unwrap();
        Database::open(f.path()).unwrap();
        let w = Writer::spawn(f.path()).unwrap();
        let reader = rusqlite::Connection::open(f.path()).unwrap();
        let pool = build_pool(&reader, &w).unwrap();
        assert_eq!(pool.len(), 1);
    }

    #[tokio::test]
    async fn pool_registers_known_provider_families() {
        let f = NamedTempFile::new().unwrap();
        Database::open(f.path()).unwrap();
        let w = Writer::spawn(f.path()).unwrap();
        creds_store::insert(
            &w,
            "anthropic".into(),
            "api_key".into(),
            "sk-a".into(),
            None,
        )
        .await
        .unwrap();
        creds_store::insert(&w, "openai".into(), "api_key".into(), "sk-o".into(), None)
            .await
            .unwrap();
        creds_store::insert(&w, "google".into(), "api_key".into(), "sk-g".into(), None)
            .await
            .unwrap();
        let reader = rusqlite::Connection::open(f.path()).unwrap();
        let pool = build_pool(&reader, &w).unwrap();
        assert_eq!(pool.len(), 4);
    }

    #[tokio::test]
    async fn pool_skips_unknown_families() {
        let f = NamedTempFile::new().unwrap();
        Database::open(f.path()).unwrap();
        let w = Writer::spawn(f.path()).unwrap();
        creds_store::insert(
            &w,
            "xai".into(),
            "api_key".into(),
            "sk".into(),
            Some("ignored".into()),
        )
        .await
        .unwrap();
        let reader = rusqlite::Connection::open(f.path()).unwrap();
        let pool = build_pool(&reader, &w).unwrap();
        assert_eq!(pool.len(), 1);
    }

    #[tokio::test]
    async fn pool_registers_openai_oauth_credential() {
        use harness_auth::oauth::TokenBundle;
        let f = NamedTempFile::new().unwrap();
        Database::open(f.path()).unwrap();
        let w = Writer::spawn(f.path()).unwrap();
        let bundle = TokenBundle {
            access_token: "at".into(),
            refresh_token: "rt".into(),
            expires_at: i64::MAX / 2,
            id_token: None,
        };
        creds_store::insert(
            &w,
            "openai".into(),
            "oauth".into(),
            serde_json::to_string(&bundle).unwrap(),
            Some("chatgpt".into()),
        )
        .await
        .unwrap();
        let reader = rusqlite::Connection::open(f.path()).unwrap();
        let pool = build_pool(&reader, &w).unwrap();
        assert_eq!(pool.len(), 2);
    }

    #[tokio::test]
    async fn pool_skips_malformed_openai_oauth() {
        let f = NamedTempFile::new().unwrap();
        Database::open(f.path()).unwrap();
        let w = Writer::spawn(f.path()).unwrap();
        creds_store::insert(&w, "openai".into(), "oauth".into(), "not-json".into(), None)
            .await
            .unwrap();
        let reader = rusqlite::Connection::open(f.path()).unwrap();
        let pool = build_pool(&reader, &w).unwrap();
        assert_eq!(pool.len(), 1);
    }
}
