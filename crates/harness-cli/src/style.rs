// IMPLEMENTS: D-202
use anstyle::{AnsiColor, RgbColor, Style};
use dialoguer::console::{Color, Style as ConsoleStyle, style};
use dialoguer::theme::ColorfulTheme;
use std::fmt::Display;
use std::io::IsTerminal;
use std::sync::OnceLock;

pub const PRIMARY: RgbColor = RgbColor(183, 167, 235);
pub const PRIMARY_DIM: RgbColor = RgbColor(140, 125, 185);

const TC_PRIMARY: Color = Color::TrueColor(183, 167, 235);
const TC_PRIMARY_DIM: Color = Color::TrueColor(140, 125, 185);

static PLAIN: OnceLock<bool> = OnceLock::new();

pub fn set_plain(plain: bool) {
    PLAIN.set(plain).ok();
}

#[must_use]
pub fn plain() -> bool {
    *PLAIN.get().unwrap_or(&false)
}

#[must_use]
pub fn supports_color() -> bool {
    if plain() {
        return false;
    }
    if std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty()) {
        return false;
    }
    std::io::stdout().is_terminal()
}

#[must_use]
pub fn supports_unicode() -> bool {
    if plain() {
        return false;
    }
    if std::env::var_os("NO_UNICODE").is_some_and(|v| !v.is_empty()) {
        return false;
    }
    if let Some(term) = std::env::var_os("TERM").and_then(|v| v.into_string().ok())
        && term == "dumb"
    {
        return false;
    }
    for var in ["LC_ALL", "LC_CTYPE", "LANG"] {
        if let Some(val) = std::env::var_os(var).and_then(|v| v.into_string().ok()) {
            let lower = val.to_lowercase();
            return lower.contains("utf-8") || lower.contains("utf8");
        }
    }
    cfg!(unix)
}

#[must_use]
pub fn primary() -> Style {
    if supports_color() {
        Style::new().fg_color(Some(PRIMARY.into()))
    } else {
        Style::new()
    }
}

#[must_use]
pub fn dim() -> Style {
    if supports_color() {
        Style::new().fg_color(Some(PRIMARY_DIM.into()))
    } else {
        Style::new()
    }
}

#[must_use]
pub fn bold_primary() -> Style {
    primary().bold()
}

#[must_use]
pub fn err() -> Style {
    if supports_color() {
        Style::new().fg_color(Some(AnsiColor::Red.into())).bold()
    } else {
        Style::new().bold()
    }
}

#[must_use]
pub fn green() -> Style {
    if supports_color() {
        Style::new().fg_color(Some(AnsiColor::Green.into()))
    } else {
        Style::new()
    }
}

#[must_use]
pub fn blue() -> Style {
    if supports_color() {
        Style::new().fg_color(Some(AnsiColor::Blue.into()))
    } else {
        Style::new()
    }
}

#[must_use]
pub fn sym_success() -> &'static str {
    if supports_unicode() { "✔" } else { "√" }
}

#[must_use]
pub fn sym_failure() -> &'static str {
    if supports_unicode() { "✖" } else { "×" }
}

#[must_use]
pub fn sym_info() -> &'static str {
    if supports_unicode() { "ℹ" } else { "i" }
}

#[must_use]
pub fn sym_spinner_frames() -> &'static [&'static str] {
    if supports_unicode() {
        &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]
    } else {
        &["|", "/", "-", "\\"]
    }
}

pub fn section(title: &str) {
    let bp = bold_primary();
    println!("{bp}{title}{bp:#}");
}

pub fn kv(label: &str, value: impl Display, width: usize) {
    let d = dim();
    let p = primary();
    let pad = width.saturating_sub(label.chars().count());
    println!(
        "  {d}{label}{d:#}{padding}  {p}{value}{p:#}",
        padding = " ".repeat(pad)
    );
}

pub fn success(msg: impl Display) {
    let g = green();
    println!("  {g}{sym}{g:#} {msg}", sym = sym_success());
}

pub fn failure(msg: impl Display) {
    let r = err();
    println!("  {r}{sym}{r:#} {msg}", sym = sym_failure());
}

pub fn info(msg: impl Display) {
    let b = blue();
    println!("  {b}{sym}{b:#} {msg}", sym = sym_info());
}

pub fn hint(msg: impl Display) {
    let d = dim();
    println!("  {d}{msg}{d:#}");
}

#[must_use]
pub fn link(text: &str, url: &str) -> String {
    if supports_color() {
        format!("\x1b]8;;{url}\x07{text}\x1b]8;;\x07")
    } else if text == url {
        text.to_string()
    } else {
        format!("{text} ({url})")
    }
}

#[must_use]
pub fn mask_key(key: &str) -> String {
    let chars: Vec<char> = key.chars().collect();
    if chars.len() <= 13 {
        return "***".into();
    }
    let head: String = chars.iter().take(8).collect();
    let tail_rev: Vec<char> = chars.iter().rev().take(5).copied().collect();
    let tail: String = tail_rev.into_iter().rev().collect();
    format!("{head}…{tail}")
}

#[must_use]
pub fn dialoguer_theme() -> ColorfulTheme {
    let primary_console = ConsoleStyle::new().fg(TC_PRIMARY);
    let dim_console = ConsoleStyle::new().fg(TC_PRIMARY_DIM);
    ColorfulTheme {
        defaults_style: dim_console.clone(),
        prompt_style: ConsoleStyle::new().bold(),
        prompt_prefix: style("?".to_string()).blue().bold(),
        prompt_suffix: style("›".to_string()).fg(TC_PRIMARY_DIM),
        success_prefix: style(sym_success().to_string()).green().bold(),
        success_suffix: style("›".to_string()).fg(TC_PRIMARY_DIM),
        error_prefix: style(sym_failure().to_string()).red().bold(),
        error_style: ConsoleStyle::new().red(),
        hint_style: dim_console,
        values_style: primary_console.clone(),
        active_item_style: primary_console.bold(),
        active_item_prefix: style("❯".to_string()).fg(TC_PRIMARY).bold(),
        inactive_item_prefix: style(" ".to_string()),
        picked_item_prefix: style(sym_success().to_string()).green().bold(),
        ..ColorfulTheme::default()
    }
}

#[must_use]
pub fn clap_styles() -> clap::builder::Styles {
    clap::builder::Styles::styled()
        .header(Style::new().bold().fg_color(Some(PRIMARY.into())))
        .usage(Style::new().bold().fg_color(Some(PRIMARY.into())))
        .literal(Style::new().fg_color(Some(PRIMARY.into())))
        .placeholder(Style::new().fg_color(Some(PRIMARY_DIM.into())))
        .error(Style::new().bold().fg_color(Some(AnsiColor::Red.into())))
        .valid(Style::new().fg_color(Some(AnsiColor::Green.into())))
        .invalid(Style::new().fg_color(Some(AnsiColor::Red.into())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dialoguer_theme_constructs() {
        let _t = dialoguer_theme();
    }

    #[test]
    fn clap_styles_constructs() {
        let _s = clap_styles();
    }

    #[test]
    fn primary_dim_distinct() {
        assert_ne!(PRIMARY, PRIMARY_DIM);
    }

    #[test]
    fn mask_key_short_returns_stars() {
        assert_eq!(mask_key(""), "***");
        assert_eq!(mask_key("short"), "***");
        let thirteen = "a".repeat(13);
        assert_eq!(mask_key(&thirteen), "***");
    }

    #[test]
    fn mask_key_long_returns_prefix_ellipsis_suffix() {
        let key = "sk-ant-aBcDeFgHiJkLmNoPqRsT";
        let masked = mask_key(key);
        assert_eq!(masked, "sk-ant-a…PqRsT");
    }

    #[test]
    fn mask_key_exactly_fourteen_masks() {
        let fourteen = "abcdefghijklmn";
        let masked = mask_key(fourteen);
        assert_eq!(masked, "abcdefgh…jklmn");
    }

    #[test]
    fn link_same_text_url_returns_url_when_no_tty() {
        let url = "https://example.com";
        let out = link(url, url);
        assert!(out.contains("example.com"));
    }

    #[test]
    fn link_different_text_shows_both_when_no_tty() {
        let out = link("click here", "https://example.com");
        assert!(out.contains("example.com"));
        assert!(out.contains("click here"));
    }

    #[test]
    fn sym_success_is_unicode_or_ascii() {
        let s = sym_success();
        assert!(s == "✔" || s == "√");
    }

    #[test]
    fn sym_failure_is_unicode_or_ascii() {
        let s = sym_failure();
        assert!(s == "✖" || s == "×");
    }
}
