// IMPLEMENTS: D-401
//! Cognitive skill metadata. Three required fields. Skills declare
//! their `cognitive_load` (1–5), accommodations they support, and
//! conditions they're explicitly contraindicated for.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CognitiveSkillMeta {
    pub cognitive_load: u8,
    pub accommodations: Vec<String>,
    pub contraindicated: Vec<String>,
}

pub fn validate_cognitive_meta(meta: &CognitiveSkillMeta) -> Result<(), &'static str> {
    if meta.cognitive_load == 0 || meta.cognitive_load > 5 {
        return Err("cognitive_load must be between 1 and 5");
    }
    if meta.accommodations.is_empty() {
        return Err("accommodations must list at least one supported aid");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta() -> CognitiveSkillMeta {
        CognitiveSkillMeta {
            cognitive_load: 3,
            accommodations: vec!["plain language".into(), "step-by-step".into()],
            contraindicated: vec!["acute crisis".into()],
        }
    }

    #[test]
    fn full_meta_validates() {
        assert!(validate_cognitive_meta(&meta()).is_ok());
    }

    #[test]
    fn out_of_range_load_rejected() {
        let mut m = meta();
        m.cognitive_load = 6;
        assert!(validate_cognitive_meta(&m).is_err());
    }

    #[test]
    fn empty_accommodations_rejected() {
        let mut m = meta();
        m.accommodations.clear();
        assert!(validate_cognitive_meta(&m).is_err());
    }
}
