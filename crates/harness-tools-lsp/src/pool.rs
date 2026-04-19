use crate::client::{LspClient, LspConfig, LspError};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Key {
    root: PathBuf,
    language: String,
}

type ServerRegistry = HashMap<String, (String, Vec<String>)>;

#[derive(Default, Clone)]
pub struct LspPool {
    clients: Arc<Mutex<HashMap<Key, Arc<LspClient>>>>,
    servers: Arc<Mutex<ServerRegistry>>,
}

impl LspPool {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn register_server(&self, language: String, command: String, args: Vec<String>) {
        self.servers.lock().await.insert(language, (command, args));
    }

    pub async fn acquire(&self, root: PathBuf, language: &str) -> Result<Arc<LspClient>, LspError> {
        let key = Key {
            root: root.clone(),
            language: language.into(),
        };
        {
            let g = self.clients.lock().await;
            if let Some(c) = g.get(&key) {
                return Ok(c.clone());
            }
        }

        let (command, args) = self
            .servers
            .lock()
            .await
            .get(language)
            .cloned()
            .ok_or_else(|| LspError::Spawn(format!("no server registered for {language}")))?;
        let client = LspClient::spawn(LspConfig {
            command,
            args,
            root,
        })
        .await?;
        let arc = Arc::new(client);
        self.clients.lock().await.insert(key, arc.clone());
        Ok(arc)
    }

    pub async fn shutdown(&self) {
        let mut g = self.clients.lock().await;
        for (_, c) in g.drain() {
            c.shutdown().await;
        }
    }
}
