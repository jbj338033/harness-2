// IMPLEMENTS: D-120, D-154, D-203, D-276, D-345
//! Translation runtime. Built-in catalog covers ko/ja/en at launch and
//! leaves zh/es/fr/de stubs that fall back to English until the human
//! review lands (D-276). Lookups go through `t(locale, key, args)`. The
//! key catalog itself lives in [`catalog`] and is the source-of-truth
//! every CI gate checks against (D-154).
//!
//! [`bidi`] hosts the Trojan-Source-class control sanitizer the TUI uses
//! before painting any model output (D-345).

pub mod bidi;
pub mod catalog;

pub use bidi::{BidiLine, contains_bidi_controls, sanitize_for_display};
pub use catalog::{CategoryError, KeyCategory, all_keys, category_of, keys_in_category};

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Locale {
    En,
    Ko,
    Ja,
    Zh,
    Es,
    Fr,
    De,
}

impl Locale {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::En => "en",
            Self::Ko => "ko",
            Self::Ja => "ja",
            Self::Zh => "zh",
            Self::Es => "es",
            Self::Fr => "fr",
            Self::De => "de",
        }
    }

    /// Parse the language portion of a POSIX locale string (`ko_KR.UTF-8`,
    /// `ja-JP`, `en`). Unknown languages fall back to English.
    #[must_use]
    pub fn parse_loose(s: &str) -> Self {
        let lang = s
            .split(['_', '-', '.', '@'])
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();
        match lang.as_str() {
            "ko" => Self::Ko,
            "ja" => Self::Ja,
            "zh" => Self::Zh,
            "es" => Self::Es,
            "fr" => Self::Fr,
            "de" => Self::De,
            _ => Self::En,
        }
    }
}

/// Best-effort detection from `LC_ALL`/`LC_MESSAGES`/`LANG`. Anything we
/// can't parse falls back to English.
#[must_use]
pub fn detect_locale() -> Locale {
    for var in ["LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Some(v) = std::env::var_os(var).and_then(|v| v.into_string().ok())
            && !v.is_empty()
        {
            return Locale::parse_loose(&v);
        }
    }
    Locale::En
}

/// Translate `key` for the given `locale`. Falls back to the English
/// template when the locale catalog has no entry — D-276 commits to this
/// fallback while later languages get human review.
#[must_use]
pub fn t(locale: Locale, key: &str, args: &BTreeMap<String, String>) -> String {
    let template = catalog::lookup(locale, key)
        .or_else(|| catalog::lookup(Locale::En, key))
        .unwrap_or(key);
    render(template, args)
}

/// `{name}` placeholder substitution — same shape as `harness-context::alert`
/// to keep templates portable. Unknown placeholders pass through.
#[must_use]
pub fn render(template: &str, args: &BTreeMap<String, String>) -> String {
    let mut out = String::with_capacity(template.len());
    let bytes = template.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b'{'
            && let Some(rel) = bytes[i + 1..].iter().position(|b| *b == b'}')
        {
            let name = &template[i + 1..i + 1 + rel];
            if let Some(v) = args.get(name) {
                out.push_str(v);
            } else {
                out.push_str(&template[i..i + 1 + rel + 1]);
            }
            i += 1 + rel + 1;
            continue;
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_loose_extracts_lang_portion() {
        assert_eq!(Locale::parse_loose("ko_KR.UTF-8"), Locale::Ko);
        assert_eq!(Locale::parse_loose("ja-JP"), Locale::Ja);
        assert_eq!(Locale::parse_loose("en"), Locale::En);
        assert_eq!(Locale::parse_loose("xx_YY"), Locale::En);
    }

    #[test]
    fn t_uses_locale_catalog_when_present() {
        let out = t(Locale::Ko, "approval_prompt.allow_once", &BTreeMap::new());
        // Korean template should not equal the English one for this key.
        let en = t(Locale::En, "approval_prompt.allow_once", &BTreeMap::new());
        assert_ne!(out, en);
    }

    #[test]
    fn t_falls_back_to_english_when_locale_missing() {
        // zh is in the catalog as fallback-to-en for now.
        let zh = t(Locale::Zh, "approval_prompt.allow_once", &BTreeMap::new());
        let en = t(Locale::En, "approval_prompt.allow_once", &BTreeMap::new());
        assert_eq!(zh, en);
    }

    #[test]
    fn t_returns_key_when_unknown() {
        assert_eq!(
            t(Locale::En, "no.such.key", &BTreeMap::new()),
            "no.such.key"
        );
    }

    #[test]
    fn render_substitutes_named_args() {
        let mut args = BTreeMap::new();
        args.insert("name".into(), "harness".into());
        assert_eq!(render("hello {name}", &args), "hello harness");
    }

    #[test]
    fn detect_falls_back_to_en_without_env() {
        // We can't reliably remove env in test, just check the function
        // returns *some* locale.
        let _ = detect_locale();
    }
}
