use anstyle::{AnsiColor, RgbColor, Style};
use dialoguer::console::{Color, Style as ConsoleStyle, style};
use dialoguer::theme::ColorfulTheme;
use std::io::IsTerminal;

pub const PRIMARY: RgbColor = RgbColor(183, 167, 235);
pub const PRIMARY_DIM: RgbColor = RgbColor(140, 125, 185);

const TC_PRIMARY: Color = Color::TrueColor(183, 167, 235);
const TC_PRIMARY_DIM: Color = Color::TrueColor(140, 125, 185);

#[must_use]
pub fn supports_color() -> bool {
    if std::env::var_os("NO_COLOR").is_some() {
        return false;
    }
    std::io::stdout().is_terminal()
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
pub fn dialoguer_theme() -> ColorfulTheme {
    let primary_console = ConsoleStyle::new().fg(TC_PRIMARY);
    let dim_console = ConsoleStyle::new().fg(TC_PRIMARY_DIM);
    ColorfulTheme {
        defaults_style: dim_console.clone(),
        prompt_style: ConsoleStyle::new().bold(),
        prompt_prefix: style("▸".to_string()).fg(TC_PRIMARY).bold(),
        prompt_suffix: style("›".to_string()).fg(TC_PRIMARY_DIM),
        success_prefix: style("✓".to_string()).fg(TC_PRIMARY).bold(),
        success_suffix: style("›".to_string()).fg(TC_PRIMARY_DIM),
        error_prefix: style("✗".to_string()).red().bold(),
        error_style: ConsoleStyle::new().red(),
        hint_style: dim_console,
        values_style: primary_console.clone(),
        active_item_style: primary_console.bold(),
        active_item_prefix: style("◆".to_string()).fg(TC_PRIMARY).bold(),
        inactive_item_prefix: style(" ".to_string()),
        picked_item_prefix: style("✓".to_string()).fg(TC_PRIMARY).bold(),
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
}
