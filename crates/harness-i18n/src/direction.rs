// IMPLEMENTS: D-344
//! Script direction helper. Closes the gap fluent-rs upstream issue #316
//! left open: callers want `dir(locale) -> Direction` so the renderer can
//! pick a flexbox direction or a cursor strategy without depending on
//! ICU4X for that single bit.

use crate::Locale;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Ltr,
    Rtl,
}

#[must_use]
pub fn dir(locale: Locale) -> Direction {
    match locale {
        Locale::En
        | Locale::Ko
        | Locale::Ja
        | Locale::Zh
        | Locale::Es
        | Locale::Fr
        | Locale::De => Direction::Ltr,
    }
}

/// Override for callers that detect strong RTL text inside an LTR locale
/// (eg. an English UI surfacing an Arabic file name) — pass the bidi
/// signal from [`crate::bidi::BidiLine::has_rtl`] to flip the cell.
#[must_use]
pub fn dir_for_text(locale: Locale, text_has_rtl: bool) -> Direction {
    if text_has_rtl {
        Direction::Rtl
    } else {
        dir(locale)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launch_locales_are_all_ltr() {
        for loc in [
            Locale::En,
            Locale::Ko,
            Locale::Ja,
            Locale::Zh,
            Locale::Es,
            Locale::Fr,
            Locale::De,
        ] {
            assert_eq!(dir(loc), Direction::Ltr, "{loc:?}");
        }
    }

    #[test]
    fn dir_for_text_promotes_to_rtl_when_text_demands_it() {
        assert_eq!(dir_for_text(Locale::En, true), Direction::Rtl);
        assert_eq!(dir_for_text(Locale::En, false), Direction::Ltr);
    }
}
