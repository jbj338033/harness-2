// IMPLEMENTS: D-424
//! Branding terminology guard. The original "분신 / bunshin" framing
//! implies an agency hand-off that East Asian readers parse very
//! differently from the Western "agent" frame — it muddies who is
//! responsible when the model misbehaves. We standardise on
//! "agent runtime" everywhere.

use serde::{Deserialize, Serialize};

pub const CANONICAL_TERM: &str = "agent runtime";

pub const BANNED_TERMS: &[&str] = &["분신", "bunshin", "Bunshin"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrandingFinding {
    pub term: String,
    pub byte_offset: usize,
}

#[must_use]
pub fn scan_for_banned_terms(text: &str) -> Vec<BrandingFinding> {
    let mut out = Vec::new();
    for term in BANNED_TERMS {
        let mut search_from = 0;
        while let Some(rel) = text[search_from..].find(term) {
            let absolute = search_from + rel;
            out.push(BrandingFinding {
                term: (*term).to_string(),
                byte_offset: absolute,
            });
            search_from = absolute + term.len();
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn benign_text_clean() {
        assert!(scan_for_banned_terms("Harness is an agent runtime.").is_empty());
    }

    #[test]
    fn korean_term_caught() {
        let f = scan_for_banned_terms("우리는 분신을 만든다");
        assert!(f.iter().any(|x| x.term == "분신"));
    }

    #[test]
    fn romanised_term_caught() {
        let f = scan_for_banned_terms("Bunshin and bunshin are both flagged.");
        assert!(f.iter().any(|x| x.term == "bunshin"));
        assert!(f.iter().any(|x| x.term == "Bunshin"));
    }

    #[test]
    fn canonical_term_is_agent_runtime() {
        assert_eq!(CANONICAL_TERM, "agent runtime");
    }
}
