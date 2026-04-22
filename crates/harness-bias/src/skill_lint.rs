// IMPLEMENTS: D-303
//! Skill linter — WinoBias substitution + axe-core enumeration.
//! WinoBias triggers on gendered occupation defaults; the axe-core
//! variant lists rule names the surface-level scanner should check
//! when the skill renders HTML.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillLintRule {
    /// WinoBias: gendered occupation default (e.g. "the doctor … he").
    WinoBiasOccupationDefault,
    /// WinoBias: pronoun resolution defaulting to majority.
    WinoBiasPronounDefault,
    /// axe-core: missing alt text on referenced images.
    AxeMissingImgAlt,
    /// axe-core: clickable element without an accessible name.
    AxeButtonNoName,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillLintFinding {
    pub rule: SkillLintRule,
    pub matched: String,
}

const PRONOUN_DEFAULTS: &[&str] = &[
    "the doctor he",
    "the nurse she",
    "the engineer he",
    "the secretary she",
    "the ceo he",
];

#[must_use]
pub fn lint_skill(skill_text: &str, html_snippets: &[&str]) -> Vec<SkillLintFinding> {
    let mut out = Vec::new();
    let lower = skill_text.to_ascii_lowercase();
    for p in PRONOUN_DEFAULTS {
        if lower.contains(p) {
            out.push(SkillLintFinding {
                rule: SkillLintRule::WinoBiasPronounDefault,
                matched: (*p).to_string(),
            });
        }
    }
    if lower.contains("the doctor") && lower.contains(" he ") {
        out.push(SkillLintFinding {
            rule: SkillLintRule::WinoBiasOccupationDefault,
            matched: "the doctor … he".to_string(),
        });
    }
    for h in html_snippets {
        let h_lower = h.to_ascii_lowercase();
        if h_lower.contains("<img") && !h_lower.contains("alt=") {
            out.push(SkillLintFinding {
                rule: SkillLintRule::AxeMissingImgAlt,
                matched: (*h).to_string(),
            });
        }
        if h_lower.contains("<button") && !h_lower.contains("aria-label") && !h_lower.contains(">")
        {
            out.push(SkillLintFinding {
                rule: SkillLintRule::AxeButtonNoName,
                matched: (*h).to_string(),
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn benign_skill_clean() {
        assert!(lint_skill("Summarises a chart.", &[]).is_empty());
    }

    #[test]
    fn gendered_default_caught() {
        let f = lint_skill("The nurse she said hi.", &[]);
        assert!(
            f.iter()
                .any(|x| matches!(x.rule, SkillLintRule::WinoBiasPronounDefault))
        );
    }

    #[test]
    fn missing_alt_caught() {
        let f = lint_skill("doc", &["<img src=\"x.png\">"]);
        assert!(
            f.iter()
                .any(|x| matches!(x.rule, SkillLintRule::AxeMissingImgAlt))
        );
    }
}
