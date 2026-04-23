// IMPLEMENTS: D-242
//! Subagent input filter — how much of the parent's context flows
//! down on Summon. AutoGen Magentic / MetaGPT SOP context-trimming
//! generalised.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SubagentInputFilter {
    Full,
    Summary { max_tokens: u32 },
    Slice { event_kinds: Vec<String> },
}

impl SubagentInputFilter {
    #[must_use]
    pub fn is_lossy(&self) -> bool {
        !matches!(self, SubagentInputFilter::Full)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_is_not_lossy() {
        assert!(!SubagentInputFilter::Full.is_lossy());
    }

    #[test]
    fn summary_is_lossy() {
        assert!(SubagentInputFilter::Summary { max_tokens: 1000 }.is_lossy());
    }

    #[test]
    fn slice_round_trips() {
        let f = SubagentInputFilter::Slice {
            event_kinds: vec!["Speak".into(), "Act".into()],
        };
        let s = serde_json::to_string(&f).unwrap();
        assert!(s.contains("\"kind\":\"slice\""));
        let back: SubagentInputFilter = serde_json::from_str(&s).unwrap();
        assert_eq!(back, f);
    }
}
