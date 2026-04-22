// IMPLEMENTS: D-441
use serde::{Deserialize, Serialize};

/// Seven-way taxonomy of local open-weights backends per D-441. Each
/// variant maps to a different on-the-wire dialect — even when several
/// share an OpenAI-compatible surface, the discovery and listing endpoints
/// differ.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OpenBackend {
    LlamaCpp,
    Ollama,
    MistralRs,
    LmStudio,
    Vllm,
    Mlx,
    Llamafile,
}

pub const OPEN_BACKENDS: &[OpenBackend] = &[
    OpenBackend::LlamaCpp,
    OpenBackend::Ollama,
    OpenBackend::MistralRs,
    OpenBackend::LmStudio,
    OpenBackend::Vllm,
    OpenBackend::Mlx,
    OpenBackend::Llamafile,
];

impl OpenBackend {
    #[must_use]
    pub fn family(self) -> &'static str {
        match self {
            Self::LlamaCpp => "llamacpp",
            Self::Ollama => "ollama",
            Self::MistralRs => "mistralrs",
            Self::LmStudio => "lmstudio",
            Self::Vllm => "vllm",
            Self::Mlx => "mlx",
            Self::Llamafile => "llamafile",
        }
    }

    #[must_use]
    pub fn default_endpoint(self) -> &'static str {
        match self {
            Self::Ollama => "http://localhost:11434",
            Self::LlamaCpp => "http://localhost:8080",
            Self::LmStudio => "http://localhost:1234",
            Self::Vllm => "http://localhost:8000",
            Self::MistralRs => "http://localhost:1234",
            Self::Mlx => "http://localhost:8080",
            Self::Llamafile => "http://localhost:8080",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn taxonomy_covers_seven_backends() {
        assert_eq!(OPEN_BACKENDS.len(), 7);
    }

    #[test]
    fn families_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for b in OPEN_BACKENDS {
            assert!(seen.insert(b.family()), "duplicate family {}", b.family());
        }
    }

    #[test]
    fn default_endpoints_are_loopback_or_localhost() {
        for b in OPEN_BACKENDS {
            let ep = b.default_endpoint();
            assert!(
                ep.contains("localhost") || ep.contains("127.0.0.1"),
                "{} default endpoint must stay local: {}",
                b.family(),
                ep
            );
        }
    }
}
