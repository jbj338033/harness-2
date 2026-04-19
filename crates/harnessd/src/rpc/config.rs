use crate::Daemon;
use crate::rpc::errors::rpc_err;
use harness_proto::ErrorCode;
use harness_rpc::{Handler, handler, parse_params};
use harness_storage::config as cfg_store;
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;

pub fn get(d: Arc<Daemon>) -> Arc<dyn Handler> {
    handler(move |p, _s| {
        let d = d.clone();
        async move {
            #[derive(Deserialize)]
            struct Params {
                key: String,
            }
            let Params { key } = parse_params(p)?;
            let reader = d
                .storage
                .readers
                .get()
                .map_err(|e| rpc_err(ErrorCode::InternalError, e))?;
            let v =
                cfg_store::get(&reader, &key).map_err(|e| rpc_err(ErrorCode::InternalError, e))?;
            Ok(json!({"key": key, "value": v}))
        }
    })
}

pub fn set(d: Arc<Daemon>) -> Arc<dyn Handler> {
    handler(move |p, _s| {
        let d = d.clone();
        async move {
            #[derive(Deserialize)]
            struct Params {
                key: String,
                value: String,
            }
            let Params { key, value } = parse_params(p)?;
            cfg_store::set(&d.storage.writer, key.clone(), value.clone())
                .await
                .map_err(|e| rpc_err(ErrorCode::InternalError, e))?;
            Ok(json!({"key": key, "value": value}))
        }
    })
}

pub fn list(d: Arc<Daemon>) -> Arc<dyn Handler> {
    handler(move |_p, _s| {
        let d = d.clone();
        async move {
            let reader = d
                .storage
                .readers
                .get()
                .map_err(|e| rpc_err(ErrorCode::InternalError, e))?;
            let all = cfg_store::list(&reader).map_err(|e| rpc_err(ErrorCode::InternalError, e))?;
            Ok(json!({
                "entries": all.into_iter().map(|(k, v)| json!({"key": k, "value": v})).collect::<Vec<_>>()
            }))
        }
    })
}

pub fn unset(d: Arc<Daemon>) -> Arc<dyn Handler> {
    handler(move |p, _s| {
        let d = d.clone();
        async move {
            #[derive(Deserialize)]
            struct Params {
                key: String,
            }
            let Params { key } = parse_params(p)?;
            let removed = cfg_store::unset(&d.storage.writer, key)
                .await
                .map_err(|e| rpc_err(ErrorCode::InternalError, e))?;
            Ok(json!({"removed": removed}))
        }
    })
}
