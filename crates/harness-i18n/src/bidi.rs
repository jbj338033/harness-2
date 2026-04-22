// IMPLEMENTS: D-345
//! Bidi safety helpers. The TUI calls [`sanitize_for_display`] on every
//! line of model / tool output before rendering — this strips the Unicode
//! formatting controls that the Trojan Source CVE-2021-42574 disclosure
//! demonstrated could be abused to make code render very differently than
//! it parses.
//!
//! D-345 also asks for proper bidi reordering. Until ratatui issue #1250
//! lands a real implementation we expose [`BidiLine`] as a thin wrapper
//! that simply records whether the input contains right-to-left text so
//! the renderer can pick a sensible cursor strategy.

use serde::{Deserialize, Serialize};

/// Unicode bidi formatting characters scrubbed from rendered text — they
/// are exactly the ones the Trojan Source paper called out as visual /
/// logical mismatches.
const STRIP_CHARS: &[char] = &[
    '\u{202A}', // LRE — Left-to-Right Embedding
    '\u{202B}', // RLE — Right-to-Left Embedding
    '\u{202C}', // PDF — Pop Directional Formatting
    '\u{202D}', // LRO — Left-to-Right Override
    '\u{202E}', // RLO — Right-to-Left Override
    '\u{2066}', // LRI — Left-to-Right Isolate
    '\u{2067}', // RLI — Right-to-Left Isolate
    '\u{2068}', // FSI — First Strong Isolate
    '\u{2069}', // PDI — Pop Directional Isolate
    '\u{200E}', // LRM — Left-to-Right Mark
    '\u{200F}', // RLM — Right-to-Left Mark
];

/// Drop every Trojan-Source-class control character from `text`. The
/// resulting string is safe to splash into a TUI cell without the visual
/// reordering attack the CVE described.
#[must_use]
pub fn sanitize_for_display(text: &str) -> String {
    text.chars().filter(|c| !STRIP_CHARS.contains(c)).collect()
}

/// Did the original input contain any of the dangerous controls?
#[must_use]
pub fn contains_bidi_controls(text: &str) -> bool {
    text.chars().any(|c| STRIP_CHARS.contains(&c))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BidiLine {
    /// Sanitized text, ready to render.
    pub display: String,
    /// True if the source contained right-to-left strong characters
    /// (Hebrew, Arabic, …) — the renderer uses this to flip caret direction.
    pub has_rtl: bool,
    /// True if any Trojan-Source-class control was stripped.
    pub had_controls: bool,
}

impl BidiLine {
    /// Build a render-safe representation of `text`.
    #[must_use]
    pub fn new(text: &str) -> Self {
        let had_controls = contains_bidi_controls(text);
        let display = sanitize_for_display(text);
        let has_rtl = display.chars().any(is_rtl);
        Self {
            display,
            has_rtl,
            had_controls,
        }
    }
}

/// Strong RTL classes per the Unicode bidi algorithm — covers Hebrew,
/// Arabic, Syriac, Thaana, NKo, Samaritan, Mandaic, and Arabic
/// Presentation Forms. Sufficient signal for the cursor-direction switch
/// we need; not a full UAX #9 classifier.
fn is_rtl(c: char) -> bool {
    let cp = c as u32;
    matches!(
        cp,
        0x0590..=0x05FF       // Hebrew
        | 0x0600..=0x06FF     // Arabic
        | 0x0700..=0x074F     // Syriac
        | 0x0780..=0x07BF     // Thaana
        | 0x07C0..=0x07FF     // NKo
        | 0x0800..=0x083F     // Samaritan
        | 0x0840..=0x085F     // Mandaic
        | 0xFB1D..=0xFDFF     // Hebrew + Arabic presentation
        | 0xFE70..=0xFEFF     // Arabic presentation B
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_strips_rlo_attack_chars() {
        let attack = "let \u{202E}revoke\u{202C} = false;";
        let safe = sanitize_for_display(attack);
        assert!(!safe.contains('\u{202E}'));
        assert!(!safe.contains('\u{202C}'));
        assert!(safe.contains("revoke"));
    }

    #[test]
    fn sanitize_passes_plain_ascii_through() {
        let s = "hello world";
        assert_eq!(sanitize_for_display(s), s);
    }

    #[test]
    fn sanitize_keeps_korean_and_japanese_text() {
        let s = "안녕하세요 こんにちは";
        assert_eq!(sanitize_for_display(s), s);
    }

    #[test]
    fn contains_bidi_controls_detects_lrm_rlm() {
        assert!(contains_bidi_controls("\u{200E}plain"));
        assert!(contains_bidi_controls("plain\u{200F}"));
        assert!(!contains_bidi_controls("plain"));
    }

    #[test]
    fn bidi_line_flags_rtl_text() {
        let line = BidiLine::new("שלום world");
        assert!(line.has_rtl, "Hebrew is RTL");
        assert!(!line.had_controls);
    }

    #[test]
    fn bidi_line_flags_arabic_as_rtl() {
        let line = BidiLine::new("مرحبا");
        assert!(line.has_rtl);
    }

    #[test]
    fn bidi_line_clears_controls_and_records_their_presence() {
        let line = BidiLine::new("foo\u{202E}bar\u{202C}");
        assert!(line.had_controls);
        assert_eq!(line.display, "foobar");
    }

    #[test]
    fn ascii_only_line_is_not_marked_rtl() {
        let line = BidiLine::new("plain ascii");
        assert!(!line.has_rtl);
        assert!(!line.had_controls);
    }
}
