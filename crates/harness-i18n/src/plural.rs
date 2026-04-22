// IMPLEMENTS: D-344
//! Locale-aware cardinal plural categories — minimal CLDR-derived rules
//! for the seven launch locales. Patterns avoid pulling ICU4X for a
//! single decision; if more locales are added the lookup graduates to
//! `icu_plurals`.

use crate::Locale;
use serde::{Deserialize, Serialize};

/// Six CLDR cardinal categories. Most locales only use a subset; English
/// for example only uses `One` and `Other`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluralCategory {
    Zero,
    One,
    Two,
    Few,
    Many,
    Other,
}

#[must_use]
pub fn cardinal(locale: Locale, n: u64) -> PluralCategory {
    match locale {
        Locale::Ko | Locale::Ja | Locale::Zh => PluralCategory::Other, // no plural inflection
        Locale::En | Locale::De | Locale::Es => {
            if n == 1 {
                PluralCategory::One
            } else {
                PluralCategory::Other
            }
        }
        Locale::Fr => {
            // CLDR: 0 and 1 are One in French
            if n == 0 || n == 1 {
                PluralCategory::One
            } else {
                PluralCategory::Other
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cjk_locales_only_use_other() {
        for loc in [Locale::Ko, Locale::Ja, Locale::Zh] {
            for n in [0, 1, 2, 100] {
                assert_eq!(cardinal(loc, n), PluralCategory::Other);
            }
        }
    }

    #[test]
    fn english_singular_for_exactly_one() {
        assert_eq!(cardinal(Locale::En, 0), PluralCategory::Other);
        assert_eq!(cardinal(Locale::En, 1), PluralCategory::One);
        assert_eq!(cardinal(Locale::En, 2), PluralCategory::Other);
    }

    #[test]
    fn french_treats_zero_as_singular() {
        assert_eq!(cardinal(Locale::Fr, 0), PluralCategory::One);
        assert_eq!(cardinal(Locale::Fr, 1), PluralCategory::One);
        assert_eq!(cardinal(Locale::Fr, 2), PluralCategory::Other);
    }

    #[test]
    fn german_and_spanish_match_english_for_basic_counts() {
        for loc in [Locale::De, Locale::Es] {
            assert_eq!(cardinal(loc, 1), PluralCategory::One);
            assert_eq!(cardinal(loc, 2), PluralCategory::Other);
        }
    }
}
