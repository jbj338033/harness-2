use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelCapability {
    Text,
    Vision,
    Tools,
    Reasoning,
    LongContext,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Model {
    pub id: String,
    pub provider: String,
    pub context_window: u32,
    #[serde(default)]
    pub capabilities: Vec<ModelCapability>,
}

#[derive(Debug, Clone, Default)]
pub struct ModelRegistry {
    models: BTreeMap<String, Model>,
}

impl ModelRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn with_builtins() -> Self {
        let mut r = Self::new();
        r.seed_builtins();
        r
    }

    pub fn insert(&mut self, model: Model) {
        self.models.insert(model.id.clone(), model);
    }

    #[must_use]
    pub fn get(&self, id: &str) -> Option<&Model> {
        self.models.get(id)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Model> {
        self.models.values()
    }

    pub fn by_provider<'a>(&'a self, provider: &'a str) -> impl Iterator<Item = &'a Model> + 'a {
        self.models.values().filter(move |m| m.provider == provider)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.models.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.models.is_empty()
    }

    pub fn ingest_ollama_tags(&mut self, json_body: &str) -> Result<usize, serde_json::Error> {
        #[derive(serde::Deserialize)]
        struct Tags {
            #[serde(default)]
            models: Vec<Tag>,
        }
        #[derive(serde::Deserialize)]
        struct Tag {
            name: String,
        }
        let parsed: Tags = serde_json::from_str(json_body)?;
        let before = self.len();
        for tag in parsed.models {
            self.insert(Model {
                id: tag.name.clone(),
                provider: "ollama".into(),
                context_window: 128_000,
                capabilities: vec![ModelCapability::Tools],
            });
        }
        Ok(self.len() - before)
    }

    fn seed_builtins(&mut self) {
        use ModelCapability::{LongContext, Reasoning, Tools, Vision};

        let builtins: &[Model] = &[
            Model {
                id: "claude-opus-4-6".into(),
                provider: "anthropic".into(),
                context_window: 1_000_000,
                capabilities: vec![Vision, Tools, Reasoning, LongContext],
            },
            Model {
                id: "claude-sonnet-4-6".into(),
                provider: "anthropic".into(),
                context_window: 1_000_000,
                capabilities: vec![Vision, Tools, Reasoning, LongContext],
            },
            Model {
                id: "claude-haiku-4-5".into(),
                provider: "anthropic".into(),
                context_window: 200_000,
                capabilities: vec![Vision, Tools],
            },
            Model {
                id: "gpt-5.4".into(),
                provider: "openai".into(),
                context_window: 1_050_000,
                capabilities: vec![Vision, Tools, Reasoning, LongContext],
            },
            Model {
                id: "o3".into(),
                provider: "openai".into(),
                context_window: 200_000,
                capabilities: vec![Tools, Reasoning],
            },
            Model {
                id: "o4-mini".into(),
                provider: "openai".into(),
                context_window: 200_000,
                capabilities: vec![Tools, Reasoning],
            },
            Model {
                id: "gemini-3.1-pro".into(),
                provider: "google".into(),
                context_window: 2_000_000,
                capabilities: vec![Vision, Tools, Reasoning, LongContext],
            },
            Model {
                id: "gemini-2.5-flash".into(),
                provider: "google".into(),
                context_window: 1_000_000,
                capabilities: vec![Vision, Tools, LongContext],
            },
        ];

        for m in builtins {
            self.insert(m.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_registry_has_entries() {
        let r = ModelRegistry::with_builtins();
        assert!(r.len() >= 5);
        assert!(r.get("claude-opus-4-6").is_some());
        assert!(r.get("gemini-3.1-pro").is_some());
    }

    #[test]
    fn insert_overwrites() {
        let mut r = ModelRegistry::new();
        r.insert(Model {
            id: "custom".into(),
            provider: "ollama".into(),
            context_window: 128_000,
            capabilities: vec![],
        });
        r.insert(Model {
            id: "custom".into(),
            provider: "ollama".into(),
            context_window: 256_000,
            capabilities: vec![],
        });
        assert_eq!(r.get("custom").unwrap().context_window, 256_000);
        assert_eq!(r.len(), 1);
    }

    #[test]
    fn ingests_ollama_tags() {
        let body = r#"{
            "models": [
                {"name": "llama3:8b"},
                {"name": "codestral:22b"}
            ]
        }"#;
        let mut r = ModelRegistry::new();
        let added = r.ingest_ollama_tags(body).unwrap();
        assert_eq!(added, 2);
        assert!(r.get("llama3:8b").is_some());
        assert_eq!(r.get("llama3:8b").unwrap().provider, "ollama");
    }

    #[test]
    fn ingests_empty_tags_body() {
        let body = r#"{"models": []}"#;
        let mut r = ModelRegistry::new();
        assert_eq!(r.ingest_ollama_tags(body).unwrap(), 0);
    }

    #[test]
    fn by_provider_filters() {
        let r = ModelRegistry::with_builtins();
        let anth: Vec<_> = r.by_provider("anthropic").collect();
        assert!(anth.iter().all(|m| m.provider == "anthropic"));
        assert!(anth.len() >= 2);
    }

    #[test]
    fn model_capability_roundtrip() {
        let caps = vec![
            ModelCapability::Vision,
            ModelCapability::Tools,
            ModelCapability::Reasoning,
        ];
        let s = serde_json::to_string(&caps).unwrap();
        let back: Vec<ModelCapability> = serde_json::from_str(&s).unwrap();
        assert_eq!(caps, back);
    }
}
