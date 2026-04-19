use crate::Daemon;
use crate::rpc::errors::rpc_err;
use crate::rpc::mappers::device_to_json;
use harness_auth::pairing::{list_devices, revoke_device};
use harness_proto::ErrorCode;
use harness_rpc::{Handler, handler, parse_params};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;

pub fn list(d: Arc<Daemon>) -> Arc<dyn Handler> {
    handler(move |_p, _s| {
        let d = d.clone();
        async move {
            let reader = d
                .storage
                .readers
                .get()
                .map_err(|e| rpc_err(ErrorCode::InternalError, e))?;
            let devices =
                list_devices(&reader).map_err(|e| rpc_err(ErrorCode::InternalError, e))?;
            Ok(json!({
                "devices": devices.iter().map(device_to_json).collect::<Vec<_>>()
            }))
        }
    })
}

pub fn revoke(d: Arc<Daemon>) -> Arc<dyn Handler> {
    handler(move |p, _s| {
        let d = d.clone();
        async move {
            #[derive(Deserialize)]
            struct Params {
                id: String,
            }
            let Params { id } = parse_params(p)?;
            let removed = revoke_device(&d.storage.writer, id)
                .await
                .map_err(|e| rpc_err(ErrorCode::InternalError, e))?;
            Ok(json!({"removed": removed}))
        }
    })
}
