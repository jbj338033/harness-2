// IMPLEMENTS: D-429
//! Liability & Insurance README — pinned 5-language set. AGPL's
//! "as-is" disclaimer is *enforceable* in the US but does not
//! survive gross-negligence challenges in many civil-law systems
//! (DE, FR, JP, KR all have explicit carve-outs). We ship a localised
//! version in each language so a non-English regulator sees the
//! same warning the English README carries.

pub const LIABILITY_LANGUAGES: &[&str] = &["en", "ko", "ja", "de", "fr"];

#[must_use]
pub fn liability_doc_path(bcp47: &str) -> String {
    format!("docs/liability/{bcp47}/LIABILITY.md")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn five_languages_pinned() {
        assert_eq!(LIABILITY_LANGUAGES.len(), 5);
    }

    #[test]
    fn includes_de_fr_jp_ko() {
        for lang in ["de", "fr", "ja", "ko"] {
            assert!(LIABILITY_LANGUAGES.contains(&lang));
        }
    }

    #[test]
    fn doc_path_points_under_docs_liability() {
        assert_eq!(liability_doc_path("ko"), "docs/liability/ko/LIABILITY.md");
    }
}
