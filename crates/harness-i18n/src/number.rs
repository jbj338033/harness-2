// IMPLEMENTS: D-344
//! Locale-aware integer formatting. Picks the decimal grouping separator
//! per CLDR for the seven launch locales — enough to render counts and
//! dollar amounts correctly without dragging in `icu_decimal`.

use crate::Locale;

#[derive(Debug, Clone, Copy)]
pub struct NumberFormat {
    pub group_separator: &'static str,
    pub decimal_separator: &'static str,
}

#[must_use]
pub fn format_for(locale: Locale) -> NumberFormat {
    match locale {
        Locale::En | Locale::Ko | Locale::Ja | Locale::Zh => NumberFormat {
            group_separator: ",",
            decimal_separator: ".",
        },
        Locale::De | Locale::Es => NumberFormat {
            group_separator: ".",
            decimal_separator: ",",
        },
        Locale::Fr => NumberFormat {
            // French uses a non-breaking space as the grouping separator.
            group_separator: "\u{202F}",
            decimal_separator: ",",
        },
    }
}

/// Format an unsigned integer with the locale's grouping rule.
#[must_use]
pub fn format_u64(n: u64, locale: Locale) -> String {
    let fmt = format_for(locale);
    let raw = n.to_string();
    insert_groups(&raw, fmt.group_separator)
}

/// Format an `i64`, prefixing with `-` when negative.
#[must_use]
pub fn format_i64(n: i64, locale: Locale) -> String {
    let fmt = format_for(locale);
    let neg = n < 0;
    let abs = n.unsigned_abs();
    let body = insert_groups(&abs.to_string(), fmt.group_separator);
    if neg { format!("-{body}") } else { body }
}

fn insert_groups(digits: &str, sep: &str) -> String {
    let bytes = digits.as_bytes();
    if bytes.len() <= 3 {
        return digits.to_string();
    }
    let mut out = String::with_capacity(bytes.len() + bytes.len() / 3);
    let first = bytes.len() % 3;
    if first > 0 {
        out.push_str(&digits[..first]);
    }
    let tail = &digits[first..];
    for (idx, chunk) in tail.as_bytes().chunks(3).enumerate() {
        if idx > 0 || first > 0 {
            out.push_str(sep);
        }
        // Digits 0-9 are ASCII so chunk slicing is always valid utf-8.
        out.push_str(std::str::from_utf8(chunk).unwrap_or("?"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn english_uses_comma_grouping() {
        assert_eq!(format_u64(1_234_567, Locale::En), "1,234,567");
    }

    #[test]
    fn german_uses_dot_grouping() {
        assert_eq!(format_u64(1_234_567, Locale::De), "1.234.567");
    }

    #[test]
    fn french_uses_non_breaking_space() {
        let s = format_u64(1_234_567, Locale::Fr);
        assert!(s.contains('\u{202F}'));
        assert!(!s.contains(','));
    }

    #[test]
    fn small_numbers_have_no_grouping() {
        assert_eq!(format_u64(42, Locale::En), "42");
        assert_eq!(format_u64(999, Locale::De), "999");
    }

    #[test]
    fn signed_negative_keeps_minus_outside_groups() {
        assert_eq!(format_i64(-1_234, Locale::En), "-1,234");
    }

    #[test]
    fn cjk_locales_match_english_grouping() {
        for loc in [Locale::Ko, Locale::Ja, Locale::Zh] {
            assert_eq!(format_u64(1_000, loc), "1,000");
        }
    }
}
