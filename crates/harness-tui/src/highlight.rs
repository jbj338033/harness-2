use crate::app::color_enabled;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use std::sync::OnceLock;
use syntect::easy::HighlightLines;
use syntect::highlighting::{Style as SyntectStyle, ThemeSet};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;

struct Cached {
    syntax_set: SyntaxSet,
    theme: syntect::highlighting::Theme,
}

fn cache() -> &'static Cached {
    static CACHE: OnceLock<Cached> = OnceLock::new();
    CACHE.get_or_init(|| {
        let syntax_set = SyntaxSet::load_defaults_newlines();
        let theme_set = ThemeSet::load_defaults();
        let theme = theme_set
            .themes
            .get("InspiredGitHub")
            .or_else(|| theme_set.themes.get("base16-ocean.dark"))
            .expect("at least one built-in theme must be available")
            .clone();
        Cached { syntax_set, theme }
    })
}

pub fn highlight(code: &str, lang: &str) -> Vec<Line<'static>> {
    if !color_enabled() {
        return code
            .lines()
            .map(|l| Line::from(Span::raw(l.to_string())))
            .collect();
    }
    let Cached { syntax_set, theme } = cache();
    let syntax = syntax_set
        .find_syntax_by_token(lang)
        .or_else(|| syntax_set.find_syntax_by_extension(lang))
        .unwrap_or_else(|| syntax_set.find_syntax_plain_text());
    let mut highlighter = HighlightLines::new(syntax, theme);
    let mut out = Vec::new();
    for line in LinesWithEndings::from(code) {
        let ranges: Vec<(SyntectStyle, &str)> = highlighter
            .highlight_line(line, syntax_set)
            .unwrap_or_default();
        let spans: Vec<Span> = ranges
            .into_iter()
            .map(|(style, text)| {
                Span::styled(
                    text.trim_end_matches('\n').to_string(),
                    to_ratatui_style(style),
                )
            })
            .collect();
        out.push(Line::from(spans));
    }
    out
}

fn to_ratatui_style(s: SyntectStyle) -> Style {
    let fg = Color::Rgb(s.foreground.r, s.foreground.g, s.foreground.b);
    Style::default().fg(fg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rust_code_produces_lines() {
        let lines = highlight("fn main() { println!(\"hi\"); }", "rs");
        assert_eq!(lines.len(), 1);
    }

    #[test]
    fn multiline_produces_multiple_lines() {
        let src = "fn a() {}\nfn b() {}\n";
        let lines = highlight(src, "rs");
        assert_eq!(lines.len(), 2);
    }

    #[test]
    fn unknown_language_falls_back_to_plain() {
        let lines = highlight("hi there", "nonexistent-lang");
        assert_eq!(lines.len(), 1);
    }

    #[test]
    fn no_color_produces_unstyled_spans() {
        super::super::app::disable_color();
        let lines = highlight("let x = 1;", "rs");
        assert_eq!(lines.len(), 1);
        for line in lines {
            for span in line.spans {
                assert!(span.style.fg.is_none());
            }
        }
    }
}
