// IMPLEMENTS: D-347
//! Locale-aware tokenizer. Skill discovery has to match a user query like
//! "Café Reservation" against a skill description that might be stored as
//! "café reservation" or even "cafe´ reservation" (decomposed). D-347
//! locks in three rules:
//!
//! 1. Normalize to NFC before any comparison.
//! 2. Apply Unicode case-fold (we use `to_lowercase`, which is the
//!    correct mapping for every script except Turkish and Lithuanian —
//!    callers needing those locales pass the right rule via the tokenize
//!    `Lang` argument).
//! 3. Strip common punctuation and split on whitespace, so "fs.read" and
//!    "fs read" tokenize the same.
//!
//! `LocalizedSkill::matches(query, locale)` is the cheap entry point the
//! daemon's skill registry will call.

use crate::Locale;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use unicode_normalization::UnicodeNormalization;

/// Locale-tagged tokenizer rule. We currently only branch on Turkish and
/// Lithuanian, which use a non-standard `i ↔ I` mapping; everything else
/// goes through the default `to_lowercase`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    Default,
    Turkish,
    Lithuanian,
}

impl From<Locale> for Lang {
    fn from(l: Locale) -> Self {
        // None of the launch locales need a special folder. The enum
        // exists so callers can pin one explicitly later without churning
        // the API.
        let _ = l;
        Self::Default
    }
}

/// Apply NFC + case fold without splitting. Useful when you need a single
/// canonical key for a string (eg. dedupe).
#[must_use]
pub fn fold(text: &str, lang: Lang) -> String {
    let nfc: String = text.nfc().collect();
    case_fold(&nfc, lang)
}

fn case_fold(text: &str, lang: Lang) -> String {
    match lang {
        Lang::Default => text.to_lowercase(),
        Lang::Turkish => text
            .chars()
            .flat_map(|c| match c {
                'I' => "ı".chars().collect::<Vec<_>>(),
                'İ' => "i".chars().collect(),
                other => other.to_lowercase().collect(),
            })
            .collect(),
        Lang::Lithuanian => text
            .chars()
            .flat_map(|c| match c {
                'I' => "i\u{0307}".chars().collect::<Vec<_>>(),
                other => other.to_lowercase().collect(),
            })
            .collect(),
    }
}

/// Tokenize after fold. Splits on every char that is not alphanumeric.
/// Empty tokens (eg. a trailing dot) are discarded.
#[must_use]
pub fn tokenize(text: &str, lang: Lang) -> Vec<String> {
    let folded = fold(text, lang);
    folded
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .map(str::to_string)
        .collect()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LocalizedSkill {
    pub name: String,
    /// `Locale -> description` map per D-347. The English entry is the
    /// fallback when the requested locale is missing.
    pub description_i18n: BTreeMap<Locale, String>,
}

impl LocalizedSkill {
    #[must_use]
    pub fn description_for(&self, locale: Locale) -> Option<&str> {
        self.description_i18n
            .get(&locale)
            .or_else(|| self.description_i18n.get(&Locale::En))
            .map(String::as_str)
    }

    /// Does the skill match this query in the chosen locale? The match
    /// succeeds when every query token is a substring of either the
    /// folded name or the folded description.
    #[must_use]
    pub fn matches(&self, query: &str, locale: Locale) -> bool {
        let lang = Lang::from(locale);
        let q_tokens = tokenize(query, lang);
        if q_tokens.is_empty() {
            return false;
        }
        let folded_name = fold(&self.name, lang);
        let folded_desc = self
            .description_for(locale)
            .map(|d| fold(d, lang))
            .unwrap_or_default();
        q_tokens
            .iter()
            .all(|tok| folded_name.contains(tok) || folded_desc.contains(tok))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fold_normalizes_combining_marks_to_nfc_first() {
        // "café" with NFD (e + combining acute) should fold to the same
        // string as the precomposed "café".
        let nfd = "cafe\u{0301}";
        let nfc = "café";
        assert_eq!(fold(nfd, Lang::Default), fold(nfc, Lang::Default));
    }

    #[test]
    fn tokenize_handles_punctuation_and_dots() {
        assert_eq!(
            tokenize("fs.read, fs.write", Lang::Default),
            vec!["fs", "read", "fs", "write"]
        );
    }

    #[test]
    fn tokenize_keeps_korean_jamo_together_in_a_token() {
        assert_eq!(tokenize("코드 리뷰", Lang::Default), vec!["코드", "리뷰"]);
    }

    #[test]
    fn turkish_lower_keeps_dotless_i() {
        // Turkish: "I" → "ı" (dotless), "İ" → "i" (dotted).
        assert_eq!(fold("ISTANBUL", Lang::Turkish), "ıstanbul");
        assert_eq!(fold("İSTANBUL", Lang::Turkish), "istanbul");
    }

    #[test]
    fn default_lower_handles_german_eszett() {
        assert_eq!(fold("Straße", Lang::Default), "straße");
    }

    fn skill() -> LocalizedSkill {
        let mut desc = BTreeMap::new();
        desc.insert(Locale::En, "Read a file from disk".into());
        desc.insert(Locale::Ko, "디스크에서 파일을 읽습니다".into());
        LocalizedSkill {
            name: "fs.read".into(),
            description_i18n: desc,
        }
    }

    #[test]
    fn matches_falls_back_to_english_for_missing_locale() {
        let s = skill();
        assert!(s.matches("read file", Locale::Ja));
    }

    #[test]
    fn matches_uses_locale_specific_description() {
        let s = skill();
        assert!(s.matches("디스크 파일", Locale::Ko));
    }

    #[test]
    fn matches_is_case_insensitive() {
        let s = skill();
        assert!(s.matches("READ FILE", Locale::En));
    }

    #[test]
    fn empty_query_does_not_match() {
        let s = skill();
        assert!(!s.matches("", Locale::En));
    }

    #[test]
    fn unrelated_query_does_not_match() {
        let s = skill();
        assert!(!s.matches("send email", Locale::En));
    }

    #[test]
    fn description_for_returns_english_fallback() {
        let mut s = skill();
        s.description_i18n.remove(&Locale::Ko);
        assert!(s.description_for(Locale::Ko).unwrap().contains("Read"));
    }
}
