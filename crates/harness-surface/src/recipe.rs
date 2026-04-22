// IMPLEMENTS: D-271
//! `Recipe` — the optional `display:` line in `SKILL.md` frontmatter.
//! When present, the Web/Mobile surface shows the recipe label
//! instead of the raw skill id. We accept it as plain UTF-8 (with
//! whitespace trimmed); colon-prefixed handles like `display: Pay
//! my rent` are parsed off the line.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Recipe {
    pub display: String,
}

/// Walk the SKILL.md frontmatter looking for a single `display:`
/// line. Returns `None` if missing or empty.
#[must_use]
pub fn parse_skill_display(skill_md_frontmatter: &str) -> Option<Recipe> {
    for line in skill_md_frontmatter.lines() {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("display:") {
            let value = rest.trim();
            if value.is_empty() {
                return None;
            }
            return Some(Recipe {
                display: value.to_string(),
            });
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_line_parsed() {
        let r = parse_skill_display("name: pay-rent\ndisplay: Pay my rent\n").unwrap();
        assert_eq!(r.display, "Pay my rent");
    }

    #[test]
    fn missing_display_returns_none() {
        assert!(parse_skill_display("name: only\n").is_none());
    }

    #[test]
    fn empty_display_returns_none() {
        assert!(parse_skill_display("display:    \n").is_none());
    }
}
