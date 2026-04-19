use crate::Daemon;
use crate::rpc::errors::rpc_err;
use harness_proto::ErrorCode;
use harness_rpc::{Handler, handler, parse_params};
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;

pub fn list(d: Arc<Daemon>) -> Arc<dyn Handler> {
    handler(move |_p, _s| {
        let d = d.clone();
        async move {
            let catalog = d
                .tools
                .skills
                .read()
                .map_err(|_| rpc_err(ErrorCode::InternalError, "skills lock poisoned"))?;
            let skills: Vec<Value> = catalog
                .iter()
                .map(|s| {
                    json!({
                        "name": s.name,
                        "description": s.description,
                        "location": s.location.display().to_string(),
                        "scope": s.scope,
                        "layout": s.layout,
                    })
                })
                .collect();
            Ok(json!({ "skills": skills }))
        }
    })
}

pub fn activate(d: Arc<Daemon>) -> Arc<dyn Handler> {
    handler(move |p, _s| {
        let d = d.clone();
        async move {
            #[derive(Deserialize)]
            struct Params {
                name: String,
            }
            let Params { name } = parse_params(p)?;
            let skill = {
                let catalog = d
                    .tools
                    .skills
                    .read()
                    .map_err(|_| rpc_err(ErrorCode::InternalError, "skills lock poisoned"))?;
                catalog
                    .get(&name)
                    .ok_or_else(|| {
                        rpc_err(ErrorCode::NotFound, format!("skill not found: {name}"))
                    })?
                    .clone()
            };
            let activation = harness_skills::activate(&skill)
                .map_err(|e| rpc_err(ErrorCode::InternalError, e))?;
            Ok(json!({
                "name": activation.name,
                "body": activation.body,
                "directory": activation.directory.display().to_string(),
                "resources": activation.resources,
            }))
        }
    })
}
