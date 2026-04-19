use crate::Daemon;
use crate::rpc::errors::rpc_err;
use harness_proto::{ErrorCode, SUPPORTED_VERSIONS};
use harness_rpc::{Handler, Sink, handler, parse_params};
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;

const VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn ping() -> Arc<dyn Handler> {
    handler(|_p: Option<Value>, _s: Sink| async move {
        Ok(json!({
            "pong": true,
            "version": VERSION,
            "protocol_versions": SUPPORTED_VERSIONS,
        }))
    })
}

pub fn negotiate() -> Arc<dyn Handler> {
    handler(|p: Option<Value>, _s: Sink| async move {
        #[derive(Deserialize)]
        struct Params {
            client_versions: Vec<u32>,
        }
        let Params { client_versions } = parse_params(p)?;
        let selected = client_versions
            .iter()
            .filter(|v| SUPPORTED_VERSIONS.contains(v))
            .copied()
            .max()
            .ok_or_else(|| {
                rpc_err(
                    ErrorCode::VersionMismatch,
                    format!(
                        "no mutually-supported protocol version (client: {client_versions:?}, server: {SUPPORTED_VERSIONS:?})"
                    ),
                )
            })?;
        Ok(json!({
            "selected": selected,
            "server_versions": SUPPORTED_VERSIONS,
        }))
    })
}

pub fn status(d: Arc<Daemon>) -> Arc<dyn Handler> {
    handler(move |_p: Option<Value>, _s: Sink| {
        let d = d.clone();
        async move {
            let models = d.llm.models.read().await.len();
            let pool = d.llm.providers.read().await;
            let pool_slots = pool.as_ref().map_or(0, |p| p.len());
            drop(pool);
            Ok(json!({
                "version": VERSION,
                "models": models,
                "provider_slots": pool_slots,
            }))
        }
    })
}

pub fn fingerprint(fp: String) -> Arc<dyn Handler> {
    handler(move |_p: Option<Value>, _s: Sink| {
        let fp = fp.clone();
        async move { Ok(json!({"fingerprint": fp})) }
    })
}
