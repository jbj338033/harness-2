use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ChatOptions {
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub stop_sequences: Option<Vec<String>>,
    #[serde(default)]
    pub provider: ProviderOptions,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProviderOptions {
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub anthropic: Option<AnthropicOptions>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub openai: Option<OpenAiOptions>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub google: Option<GoogleOptions>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub ollama: Option<OllamaOptions>,
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AnthropicOptions {
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub thinking: Option<ThinkingConfig>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub cache_control: Option<CacheControl>,
    #[serde(default, flatten, skip_serializing_if = "Map::is_empty")]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OpenAiOptions {
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub reasoning_effort: Option<ReasoningEffort>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub response_format: Option<ResponseFormat>,
    #[serde(default, flatten, skip_serializing_if = "Map::is_empty")]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GoogleOptions {
    #[serde(default, flatten, skip_serializing_if = "Map::is_empty")]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OllamaOptions {
    #[serde(default, flatten, skip_serializing_if = "Map::is_empty")]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThinkingConfig {
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub budget_tokens: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheControl {
    #[serde(rename = "type")]
    pub kind: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReasoningEffort {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResponseFormat {
    Text,
    JsonObject,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn empty_options_serialize_minimally() {
        let opts = ChatOptions::default();
        let s = serde_json::to_string(&opts).unwrap();
        assert!(!s.contains("temperature"));
        assert!(!s.contains("max_tokens"));
        assert!(s.contains("\"provider\""));
    }

    #[test]
    fn anthropic_options_roundtrip() {
        let opts = ChatOptions {
            temperature: Some(0.7),
            provider: ProviderOptions {
                anthropic: Some(AnthropicOptions {
                    thinking: Some(ThinkingConfig {
                        enabled: true,
                        budget_tokens: Some(10_000),
                    }),
                    cache_control: Some(CacheControl {
                        kind: "ephemeral".into(),
                    }),
                    ..Default::default()
                }),
                ..Default::default()
            },
            ..Default::default()
        };
        let s = serde_json::to_string(&opts).unwrap();
        let back: ChatOptions = serde_json::from_str(&s).unwrap();
        let a = back.provider.anthropic.unwrap();
        assert!(a.thinking.unwrap().enabled);
    }

    #[test]
    fn anthropic_extra_flattens() {
        let opts = AnthropicOptions {
            thinking: None,
            cache_control: None,
            extra: {
                let mut m = Map::new();
                m.insert("beta_feature".into(), json!("enabled"));
                m
            },
        };
        let s = serde_json::to_string(&opts).unwrap();
        assert!(s.contains("\"beta_feature\":\"enabled\""));
        assert!(!s.contains("\"extra\""));

        let back: AnthropicOptions = serde_json::from_str(&s).unwrap();
        assert_eq!(back.extra.get("beta_feature").unwrap(), &json!("enabled"));
    }

    #[test]
    fn multiple_provider_options_coexist() {
        let opts = ChatOptions {
            provider: ProviderOptions {
                anthropic: Some(AnthropicOptions {
                    cache_control: Some(CacheControl {
                        kind: "ephemeral".into(),
                    }),
                    ..Default::default()
                }),
                openai: Some(OpenAiOptions {
                    reasoning_effort: Some(ReasoningEffort::High),
                    ..Default::default()
                }),
                ..Default::default()
            },
            ..Default::default()
        };
        let s = serde_json::to_string(&opts).unwrap();
        assert!(s.contains("ephemeral"));
        assert!(s.contains("high"));
    }

    #[test]
    fn reasoning_effort_serde() {
        let r = ReasoningEffort::High;
        assert_eq!(serde_json::to_string(&r).unwrap(), "\"high\"");
        let back: ReasoningEffort = serde_json::from_str("\"low\"").unwrap();
        assert_eq!(back, ReasoningEffort::Low);
    }

    #[test]
    fn unknown_provider_via_extra() {
        let mut po = ProviderOptions::default();
        po.extra.insert("xai".into(), json!({"grok_mode": "fun"}));
        let s = serde_json::to_string(&po).unwrap();
        assert!(s.contains("\"xai\""));
        let back: ProviderOptions = serde_json::from_str(&s).unwrap();
        assert!(back.extra.contains_key("xai"));
    }
}
