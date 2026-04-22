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

// IMPLEMENTS: D-445
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Model {
    pub id: String,
    pub provider: String,
    pub context_window: u32,
    #[serde(default)]
    pub capabilities: Vec<ModelCapability>,
    /// SPDX licence identifier (`Apache-2.0`, `Proprietary`, …) — required
    /// for compliance manifests.
    #[serde(default)]
    pub license: String,
    /// ISO-3166 alpha-2 country of model origin (`US`, `CN`, `KR`, …) —
    /// drives US BIS AI Diffusion + KR AI Basic Act gating.
    #[serde(default)]
    pub origin_country: String,
    /// US BIS AI Diffusion tier (1=most restricted, 3=least). `None` = not
    /// in scope.
    #[serde(default)]
    pub bis_tier: Option<u8>,
    /// EU AI Act Article 53 GPAI exemption (open-weight models with public
    /// weights and architecture).
    #[serde(default)]
    pub eu_art53_exempt: bool,
    /// Korean AI Basic Act "high-impact" classification.
    #[serde(default)]
    pub kr_high_impact: bool,
    /// Free-form safety profile tag — eg. `instruction_only`,
    /// `alignment_tuned`, `unconstrained_research`.
    #[serde(default)]
    pub safety_profile: String,
    /// Tool calling wire format — eg. `anthropic_xml`, `openai_json`,
    /// `gemini_oneof`, `none`.
    #[serde(default)]
    pub tool_format: String,
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
                license: "Various-OpenWeights".into(),
                origin_country: "XX".into(),
                bis_tier: None,
                eu_art53_exempt: true,
                kr_high_impact: false,
                safety_profile: "instruction_only".into(),
                tool_format: "openai_json".into(),
            });
        }
        Ok(self.len() - before)
    }

    fn seed_builtins(&mut self) {
        use ModelCapability::{LongContext, Reasoning, Tools, Vision};

        let anthropic = |id: &str, ctx: u32, caps: Vec<ModelCapability>| Model {
            id: id.into(),
            provider: "anthropic".into(),
            context_window: ctx,
            capabilities: caps,
            license: "Proprietary".into(),
            origin_country: "US".into(),
            bis_tier: Some(2),
            eu_art53_exempt: false,
            kr_high_impact: true,
            safety_profile: "alignment_tuned".into(),
            tool_format: "anthropic_xml".into(),
        };
        let openai = |id: &str, ctx: u32, caps: Vec<ModelCapability>| Model {
            id: id.into(),
            provider: "openai".into(),
            context_window: ctx,
            capabilities: caps,
            license: "Proprietary".into(),
            origin_country: "US".into(),
            bis_tier: Some(2),
            eu_art53_exempt: false,
            kr_high_impact: true,
            safety_profile: "alignment_tuned".into(),
            tool_format: "openai_json".into(),
        };
        let google = |id: &str, ctx: u32, caps: Vec<ModelCapability>| Model {
            id: id.into(),
            provider: "google".into(),
            context_window: ctx,
            capabilities: caps,
            license: "Proprietary".into(),
            origin_country: "US".into(),
            bis_tier: Some(2),
            eu_art53_exempt: false,
            kr_high_impact: true,
            safety_profile: "alignment_tuned".into(),
            tool_format: "gemini_oneof".into(),
        };

        let builtins: Vec<Model> = vec![
            anthropic(
                "claude-opus-4-6",
                1_000_000,
                vec![Vision, Tools, Reasoning, LongContext],
            ),
            anthropic(
                "claude-sonnet-4-6",
                1_000_000,
                vec![Vision, Tools, Reasoning, LongContext],
            ),
            anthropic("claude-haiku-4-5", 200_000, vec![Vision, Tools]),
            openai(
                "gpt-5.4",
                1_050_000,
                vec![Vision, Tools, Reasoning, LongContext],
            ),
            openai("o3", 200_000, vec![Tools, Reasoning]),
            openai("o4-mini", 200_000, vec![Tools, Reasoning]),
            google(
                "gemini-3.1-pro",
                2_000_000,
                vec![Vision, Tools, Reasoning, LongContext],
            ),
            google(
                "gemini-2.5-flash",
                1_000_000,
                vec![Vision, Tools, LongContext],
            ),
        ];

        for m in &builtins {
            self.insert(m.clone());
        }
    }

    /// Load extra models from a `model_registry.toml` manifest. Per D-445
    /// the file lists `[[models]]` tables with the full compliance metadata.
    pub fn ingest_toml_manifest(&mut self, body: &str) -> Result<usize, toml::de::Error> {
        #[derive(serde::Deserialize)]
        struct Manifest {
            #[serde(default)]
            models: Vec<Model>,
        }
        let parsed: Manifest = toml::from_str(body)?;
        let before = self.len();
        for m in parsed.models {
            self.insert(m);
        }
        Ok(self.len() - before)
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
            ..Model::default()
        });
        r.insert(Model {
            id: "custom".into(),
            provider: "ollama".into(),
            context_window: 256_000,
            ..Model::default()
        });
        assert_eq!(r.get("custom").unwrap().context_window, 256_000);
        assert_eq!(r.len(), 1);
    }

    #[test]
    fn builtin_models_carry_compliance_metadata() {
        let r = ModelRegistry::with_builtins();
        let anthropic = r.get("claude-opus-4-6").unwrap();
        assert_eq!(anthropic.origin_country, "US");
        assert_eq!(anthropic.tool_format, "anthropic_xml");
        assert_eq!(anthropic.license, "Proprietary");
        assert!(
            anthropic.kr_high_impact,
            "US frontier models fall under kr high impact"
        );
        assert_eq!(anthropic.bis_tier, Some(2));

        let openai = r.get("o3").unwrap();
        assert_eq!(openai.tool_format, "openai_json");

        let google = r.get("gemini-3.1-pro").unwrap();
        assert_eq!(google.tool_format, "gemini_oneof");
    }

    #[test]
    fn toml_manifest_loads_compliance_fields() {
        let body = r#"
            [[models]]
            id = "mistral-large"
            provider = "local"
            context_window = 128000
            license = "Apache-2.0"
            origin_country = "FR"
            eu_art53_exempt = true
            safety_profile = "instruction_only"
            tool_format = "openai_json"
        "#;
        let mut r = ModelRegistry::new();
        let added = r.ingest_toml_manifest(body).unwrap();
        assert_eq!(added, 1);
        let m = r.get("mistral-large").unwrap();
        assert_eq!(m.license, "Apache-2.0");
        assert_eq!(m.origin_country, "FR");
        assert!(m.eu_art53_exempt);
        assert!(!m.kr_high_impact);
        assert!(m.bis_tier.is_none());
    }

    #[test]
    fn toml_manifest_rejects_garbage() {
        let mut r = ModelRegistry::new();
        assert!(r.ingest_toml_manifest("not =valid= toml{").is_err());
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
