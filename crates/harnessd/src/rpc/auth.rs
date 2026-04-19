use crate::Daemon;
use crate::rpc::errors::{map_storage_err, rpc_err};
use crate::rpc::mappers::preview_secret;
use harness_proto::ErrorCode;
use harness_rpc::{Handler, handler, parse_params};
use harness_storage::{config as cfg_store, credentials as creds_store};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;
use tracing::{info, warn};

pub fn creds_add(d: Arc<Daemon>) -> Arc<dyn Handler> {
    handler(move |p, _s| {
        let d = d.clone();
        async move {
            #[derive(Deserialize)]
            struct Params {
                provider: String,
                value: String,
                #[serde(default)]
                kind: Option<String>,
                #[serde(default)]
                label: Option<String>,
            }
            let Params {
                provider,
                value,
                kind,
                label,
            } = parse_params(p)?;
            let id = creds_store::insert(
                &d.storage.writer,
                provider.clone(),
                kind.unwrap_or_else(|| "api_key".into()),
                value,
                label,
            )
            .await
            .map_err(|e| rpc_err(ErrorCode::InternalError, e))?;
            rebuild_pool(&d).await;
            ensure_default_model(&d).await?;
            info!(provider = %provider, "added credential");
            Ok(json!({"id": id}))
        }
    })
}

pub fn creds_list(d: Arc<Daemon>) -> Arc<dyn Handler> {
    handler(move |_p, _s| {
        let d = d.clone();
        async move {
            let reader = d
                .storage
                .readers
                .get()
                .map_err(|e| rpc_err(ErrorCode::InternalError, e))?;
            let all =
                creds_store::list(&reader).map_err(|e| rpc_err(ErrorCode::InternalError, e))?;
            Ok(json!({
                "credentials": all.into_iter().map(|c| json!({
                    "id": c.id,
                    "provider": c.provider,
                    "kind": c.kind,
                    "label": c.label,
                    "created_at": c.created_at,
                    "value_preview": preview_secret(&c.value),
                })).collect::<Vec<_>>()
            }))
        }
    })
}

pub fn creds_delete(d: Arc<Daemon>) -> Arc<dyn Handler> {
    handler(move |p, _s| {
        let d = d.clone();
        async move {
            #[derive(Deserialize)]
            struct Params {
                id: String,
            }
            let Params { id } = parse_params(p)?;

            let reader = d
                .storage
                .readers
                .get()
                .map_err(|e| rpc_err(ErrorCode::InternalError, e))?;
            if let Some(row) = creds_store::get(&reader, &id).map_err(map_storage_err)?
                && row.provider == "openai"
                && row.kind == "oauth"
                && let Ok(bundle) =
                    serde_json::from_str::<harness_auth::oauth::TokenBundle>(&row.value)
                && let Err(e) = harness_auth::oauth::revoke(&bundle.refresh_token).await
            {
                warn!(%e, "openai oauth revoke failed (proceeding with local delete)");
            }
            drop(reader);

            let n = creds_store::delete(&d.storage.writer, id)
                .await
                .map_err(|e| rpc_err(ErrorCode::InternalError, e))?;
            rebuild_pool(&d).await;
            Ok(json!({"deleted": n}))
        }
    })
}

pub fn pair_new(d: Arc<Daemon>) -> Arc<dyn Handler> {
    handler(move |_p, _s| {
        let d = d.clone();
        async move {
            let code = d.security.pairing.new_code();
            Ok(json!({
                "code": code,
                "fingerprint": d.security.tls_fingerprint.clone(),
                "port": d.network.ws_port,
            }))
        }
    })
}

pub async fn rebuild_pool(d: &Arc<Daemon>) {
    let reader = match d.storage.readers.get() {
        Ok(c) => c,
        Err(e) => {
            warn!(error = %e, "could not open db to rebuild pool");
            return;
        }
    };
    match crate::providers::build_pool(&reader, &d.storage.writer) {
        Ok(p) => {
            *d.llm.providers.write().await = Some(Arc::new(p));
        }
        Err(e) => warn!(error = %e, "pool rebuild failed"),
    }
}

async fn ensure_default_model(d: &Arc<Daemon>) -> Result<(), harness_proto::ErrorObject> {
    let default_was_empty = d
        .llm
        .default_model
        .read()
        .map_err(|_| rpc_err(ErrorCode::InternalError, "default_model lock poisoned"))?
        .is_empty();
    if !default_was_empty {
        return Ok(());
    }
    let reader = d
        .storage
        .readers
        .get()
        .map_err(|e| rpc_err(ErrorCode::InternalError, e))?;
    let Some(suggested) = crate::providers::suggest_default_model(&reader)
        .map_err(|e| rpc_err(ErrorCode::InternalError, e))?
    else {
        return Ok(());
    };
    {
        let mut guard = d
            .llm
            .default_model
            .write()
            .map_err(|_| rpc_err(ErrorCode::InternalError, "default_model lock poisoned"))?;
        suggested.clone_into(&mut guard);
    }
    cfg_store::set(&d.storage.writer, "default_model".into(), suggested.clone())
        .await
        .map_err(|e| rpc_err(ErrorCode::InternalError, e))?;
    info!(model = %suggested, "default_model auto-set after first credential");
    Ok(())
}
