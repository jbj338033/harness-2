use crate::Daemon;
use crate::rpc::errors::rpc_err;
use harness_core::SessionId;
use harness_proto::ErrorCode;
use harness_rpc::{Handler, handler, parse_params};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;

pub fn respond(d: Arc<Daemon>) -> Arc<dyn Handler> {
    handler(move |p, _s| {
        let d = d.clone();
        async move {
            #[derive(Deserialize)]
            struct Params {
                request_id: String,
                decision: String,
                #[serde(default)]
                pattern: Option<String>,
                #[serde(default)]
                session_id: Option<String>,
            }
            let Params {
                request_id,
                decision,
                pattern,
                session_id,
            } = parse_params(p)?;

            if let Some(pat) = pattern.as_deref() {
                match decision.as_str() {
                    "allow_session" => {
                        let sid = session_id.as_deref().and_then(|s| SessionId::parse(s).ok());
                        harness_storage::approvals::insert(
                            &d.storage.writer,
                            sid,
                            pat.to_string(),
                            harness_storage::approvals::Scope::Session,
                            None,
                        )
                        .await
                        .map_err(|e| rpc_err(ErrorCode::InternalError, e))?;
                    }
                    "allow_global" => {
                        harness_storage::approvals::insert(
                            &d.storage.writer,
                            None,
                            pat.to_string(),
                            harness_storage::approvals::Scope::Global,
                            None,
                        )
                        .await
                        .map_err(|e| rpc_err(ErrorCode::InternalError, e))?;
                    }
                    _ => {}
                }
            }

            let waker = {
                let mut map = d
                    .tools
                    .pending_approvals
                    .lock()
                    .map_err(|_| rpc_err(ErrorCode::InternalError, "approval lock poisoned"))?;
                map.remove(&request_id)
            };
            if let Some(tx) = waker {
                tx.send(decision.clone()).ok();
            }

            Ok(json!({ "acknowledged": true, "decision": decision }))
        }
    })
}
