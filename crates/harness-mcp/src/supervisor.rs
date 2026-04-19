use crate::client::{ManagedServer, McpServerConfig};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

#[derive(Default, Clone)]
pub struct Supervisor {
    servers: Arc<RwLock<HashMap<String, Arc<ManagedServer>>>>,
    watchers: Arc<RwLock<Vec<JoinHandle<()>>>>,
    shutdown: CancellationToken,
}

impl Supervisor {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn start(&self, config: McpServerConfig) -> Result<(), crate::McpError> {
        let first = Arc::new(ManagedServer::spawn(config.clone()).await?);
        self.servers
            .write()
            .await
            .insert(config.name.clone(), first.clone());
        info!(name = %config.name, "mcp server started");

        let servers = self.servers.clone();
        let shutdown = self.shutdown.clone();
        let name = config.name.clone();
        let cfg = config.clone();
        let mut current = first;
        let watcher = tokio::spawn(async move {
            let mut respawns: u32 = 0;
            loop {
                loop {
                    if shutdown.is_cancelled() {
                        return;
                    }
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    if !server_alive(&current).await {
                        break;
                    }
                }
                warn!(server = %name, respawns, "mcp server died; respawning");
                servers.write().await.remove(&name);

                let delay = Duration::from_millis(200_u64 + u64::from(respawns.min(10)) * 200);
                tokio::time::sleep(delay).await;

                match ManagedServer::spawn(cfg.clone()).await {
                    Ok(server) => {
                        let arc = Arc::new(server);
                        servers.write().await.insert(name.clone(), arc.clone());
                        current = arc;
                        respawns = respawns.saturating_add(1);
                    }
                    Err(e) => {
                        warn!(server = %name, error = %e, "mcp respawn failed; giving up");
                        return;
                    }
                }
            }
        });
        self.watchers.write().await.push(watcher);
        Ok(())
    }

    pub async fn get(&self, name: &str) -> Option<Arc<ManagedServer>> {
        self.servers.read().await.get(name).cloned()
    }

    pub async fn list(&self) -> Vec<(String, Arc<ManagedServer>)> {
        self.servers
            .read()
            .await
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }

    pub async fn shutdown(&self) {
        self.shutdown.cancel();
        let names: Vec<_> = self.servers.read().await.keys().cloned().collect();
        for name in names {
            if let Some(s) = self.servers.write().await.remove(&name) {
                s.shutdown().await;
            }
        }
        let handles: Vec<_> = self.watchers.write().await.drain(..).collect();
        for h in handles {
            h.await.ok();
        }
    }

    pub async fn shutdown_one(&self, name: &str) -> bool {
        let removed = self.servers.write().await.remove(name);
        if let Some(server) = removed {
            server.shutdown().await;
            info!(name, "mcp server shut down (per request)");
            true
        } else {
            false
        }
    }
}

async fn server_alive(server: &ManagedServer) -> bool {
    match server.call_tool("__ping__", serde_json::json!({})).await {
        Ok(_) | Err(crate::McpError::Server(_)) => true,
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::fs;
    use std::io::Write;
    use tempfile::TempDir;

    fn fake_server_script(tmp: &TempDir) -> std::path::PathBuf {
        let path = tmp.path().join("fake.sh");
        let mut f = fs::File::create(&path).unwrap();
        f.write_all(
            br#"#!/usr/bin/env bash
while IFS= read -r line; do
    method=$(printf '%s' "$line" | sed -n 's/.*"method":"\([^"]*\)".*/\1/p')
    id=$(printf '%s' "$line" | sed -n 's/.*"id":\([0-9]*\).*/\1/p')
    case "$method" in
        initialize)
            printf '{"jsonrpc":"2.0","id":%s,"result":{"protocolVersion":"2024-11-05","capabilities":{},"serverInfo":{"name":"fake","version":"0.0.1"}}}\n' "$id"
            ;;
        tools/list)
            printf '{"jsonrpc":"2.0","id":%s,"result":{"tools":[{"name":"echo","description":"echo","inputSchema":{"type":"object"}}]}}\n' "$id"
            ;;
        tools/call)
            printf '{"jsonrpc":"2.0","id":%s,"result":{"content":[{"type":"text","text":"ok"}]}}\n' "$id"
            ;;
        *)
            :
            ;;
    esac
done
"#,
        )
        .unwrap();
        drop(f);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut p = fs::metadata(&path).unwrap().permissions();
            p.set_mode(0o755);
            fs::set_permissions(&path, p).unwrap();
        }
        path
    }

    #[tokio::test]
    async fn start_register_and_call() {
        let tmp = TempDir::new().unwrap();
        let path = fake_server_script(&tmp);
        let sup = Supervisor::new();
        sup.start(McpServerConfig {
            name: "fake".into(),
            command: "bash".into(),
            args: vec![path.to_string_lossy().into()],
            cwd: None,
        })
        .await
        .unwrap();

        let server = sup.get("fake").await.unwrap();
        let _result = server.call_tool("echo", json!({})).await.unwrap();
        sup.shutdown().await;
    }

    #[tokio::test]
    async fn shutdown_is_idempotent() {
        let tmp = TempDir::new().unwrap();
        let path = fake_server_script(&tmp);
        let sup = Supervisor::new();
        sup.start(McpServerConfig {
            name: "x".into(),
            command: "bash".into(),
            args: vec![path.to_string_lossy().into()],
            cwd: None,
        })
        .await
        .unwrap();
        sup.shutdown().await;
        sup.shutdown().await;
    }
}
