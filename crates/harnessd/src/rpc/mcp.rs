use crate::Daemon;
use crate::rpc::errors::rpc_err;
use harness_mcp::{McpServerConfig, McpTool, Supervisor as McpSupervisor};
use harness_proto::ErrorCode;
use harness_rpc::{Handler, handler, parse_params};
use harness_storage::{ReaderPool, config as cfg_store};
use harness_tools::Registry;
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;
use tracing::{info, warn};

const KEY_PREFIX: &str = "mcp.server.";

pub fn add(d: Arc<Daemon>) -> Arc<dyn Handler> {
    handler(move |p, _s| {
        let d = d.clone();
        async move {
            #[derive(Deserialize)]
            struct Params {
                name: String,
                command: String,
                #[serde(default)]
                args: Vec<String>,
                #[serde(default)]
                env: std::collections::BTreeMap<String, String>,
            }
            let Params {
                name,
                command,
                args,
                env,
            } = parse_params(p)?;
            if name.is_empty() || command.is_empty() {
                return Err(rpc_err(
                    ErrorCode::InvalidParams,
                    "mcp.add requires non-empty `name` and `command`",
                ));
            }
            let body = json!({
                "command": command,
                "args": args,
                "env": env,
            })
            .to_string();
            cfg_store::set(&d.storage.writer, key(&name), body.clone())
                .await
                .map_err(|e| rpc_err(ErrorCode::InternalError, e))?;
            let started = if let Some(cfg) = parse_config(&name, &body) {
                spawn_server(&d.tools.mcp_supervisor, &d.tools.registry, cfg).await
            } else {
                warn!(server = %name, "could not parse just-saved mcp config; skipping spawn");
                false
            };
            Ok(json!({ "added": name, "started": started }))
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
            let entries =
                cfg_store::list(&reader).map_err(|e| rpc_err(ErrorCode::InternalError, e))?;
            let servers: Vec<Value> = entries
                .into_iter()
                .filter_map(|(k, v)| {
                    let name = k.strip_prefix(KEY_PREFIX)?.to_string();
                    let parsed: Value = serde_json::from_str(&v)
                        .unwrap_or_else(|_| json!({ "command": v, "args": [], "env": {} }));
                    Some(json!({
                        "name": name,
                        "command": parsed.get("command").cloned().unwrap_or(Value::Null),
                        "args": parsed.get("args").cloned().unwrap_or(json!([])),
                        "env": parsed.get("env").cloned().unwrap_or(json!({})),
                    }))
                })
                .collect();
            Ok(json!({ "servers": servers }))
        }
    })
}

pub fn remove(d: Arc<Daemon>) -> Arc<dyn Handler> {
    handler(move |p, _s| {
        let d = d.clone();
        async move {
            #[derive(Deserialize)]
            struct Params {
                name: String,
            }
            let Params { name } = parse_params(p)?;
            cfg_store::unset(&d.storage.writer, key(&name))
                .await
                .map_err(|e| rpc_err(ErrorCode::InternalError, e))?;
            let was_running = d.tools.mcp_supervisor.shutdown_one(&name).await;
            let dropped = unregister_tools(&d.tools.registry, &name);
            Ok(json!({
                "removed": name,
                "stopped": was_running,
                "dropped_tools": dropped,
            }))
        }
    })
}

pub fn parse_config(name: &str, body: &str) -> Option<McpServerConfig> {
    let v: Value = serde_json::from_str(body).ok()?;
    let command = v.get("command")?.as_str()?.to_string();
    let args = v
        .get("args")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    Some(McpServerConfig {
        name: name.to_string(),
        command,
        args,
        cwd: None,
    })
}

pub async fn spawn_server(
    supervisor: &McpSupervisor,
    registry: &Registry,
    cfg: McpServerConfig,
) -> bool {
    let name = cfg.name.clone();
    match supervisor.start(cfg).await {
        Ok(()) => {
            if let Some(server) = supervisor.get(&name).await {
                for remote in server.tools().to_vec() {
                    let mcp_tool = McpTool::new(&name, remote, server.clone());
                    registry.register(Arc::new(mcp_tool));
                }
                info!(server = %name, "mcp server registered");
                true
            } else {
                warn!(server = %name, "mcp server vanished immediately after start");
                false
            }
        }
        Err(e) => {
            warn!(server = %name, error = %e, "mcp server failed to start; skipping");
            false
        }
    }
}

pub fn unregister_tools(registry: &Registry, prefix: &str) -> usize {
    let p = format!("{prefix}::");
    let names: Vec<String> = registry
        .names()
        .into_iter()
        .filter(|n| n.starts_with(&p))
        .collect();
    let count = names.len();
    for n in names {
        registry.unregister(&n);
    }
    count
}

pub async fn boot_all(readers: &ReaderPool, supervisor: &McpSupervisor, registry: &Registry) {
    let reader = match readers.get() {
        Ok(r) => r,
        Err(e) => {
            warn!(error = %e, "could not open reader for mcp boot");
            return;
        }
    };
    let entries = match cfg_store::list(&reader) {
        Ok(rows) => rows,
        Err(e) => {
            warn!(error = %e, "could not enumerate mcp.server.* configs");
            return;
        }
    };
    drop(reader);
    for (k, value) in entries {
        let Some(name) = k.strip_prefix(KEY_PREFIX) else {
            continue;
        };
        let Some(cfg) = parse_config(name, &value) else {
            warn!(server = %name, "malformed mcp.server.* config; skipping");
            continue;
        };
        spawn_server(supervisor, registry, cfg).await;
    }
}

fn key(name: &str) -> String {
    format!("{KEY_PREFIX}{name}")
}
