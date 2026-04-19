use crate::app::App;
use crate::commands::COMMANDS;

pub const BUILTIN_GLYPH: &str = "▸";
pub const SKILL_GLYPH: &str = "◆";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionItem {
    pub value: String,
    pub description: String,
    pub source: CompletionSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionSource {
    Builtin,
    Skill,
}

impl CompletionSource {
    #[must_use]
    pub const fn glyph(self) -> &'static str {
        match self {
            CompletionSource::Builtin => BUILTIN_GLYPH,
            CompletionSource::Skill => SKILL_GLYPH,
        }
    }
}

pub const MAX_ITEMS: usize = 8;

#[must_use]
pub fn candidates(app: &App, input: &str) -> Vec<CompletionItem> {
    let Some(rest) = input.strip_prefix('/') else {
        return Vec::new();
    };
    if rest.contains(' ') {
        return Vec::new();
    }
    let prefix = rest;
    let mut out: Vec<CompletionItem> = Vec::new();

    for c in COMMANDS {
        if starts_with_ci(c.name, prefix) {
            out.push(CompletionItem {
                value: c.name.to_string(),
                description: c.description.to_string(),
                source: CompletionSource::Builtin,
            });
        }
    }
    for name in &app.skills {
        if starts_with_ci(name, prefix) {
            if COMMANDS.iter().any(|c| c.name == name) {
                continue;
            }
            out.push(CompletionItem {
                value: name.clone(),
                description: "activate skill".to_string(),
                source: CompletionSource::Skill,
            });
        }
    }

    out.sort_by(|a, b| a.value.cmp(&b.value));
    if out.len() > MAX_ITEMS {
        out.truncate(MAX_ITEMS);
    }
    out
}

#[must_use]
pub fn common_prefix<'a>(items: impl IntoIterator<Item = &'a str>) -> String {
    let mut iter = items.into_iter();
    let Some(first) = iter.next() else {
        return String::new();
    };
    let mut acc: &str = first;
    for s in iter {
        acc = longest_common(acc, s);
        if acc.is_empty() {
            break;
        }
    }
    acc.to_string()
}

fn longest_common<'a>(a: &'a str, b: &str) -> &'a str {
    let mut byte_end = 0usize;
    for ((ai, ac), bc) in a.char_indices().zip(b.chars()) {
        if ac == bc {
            byte_end = ai + ac.len_utf8();
        } else {
            return &a[..byte_end.min(ai)];
        }
    }
    let shorter = a.len().min(b.len());
    &a[..byte_end.min(shorter)]
}

fn starts_with_ci(haystack: &str, prefix: &str) -> bool {
    if prefix.is_empty() {
        return true;
    }
    haystack
        .chars()
        .zip(prefix.chars())
        .all(|(h, p)| h.eq_ignore_ascii_case(&p))
        && haystack.chars().count() >= prefix.chars().count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_when_no_slash() {
        let app = App::new("0.1.0");
        assert!(candidates(&app, "hello").is_empty());
        assert!(candidates(&app, "").is_empty());
    }

    #[test]
    fn empty_after_first_space() {
        let app = App::new("0.1.0");
        assert!(candidates(&app, "/resume abc").is_empty());
    }

    #[test]
    fn slash_alone_lists_everything_up_to_cap() {
        let app = App::new("0.1.0");
        let items = candidates(&app, "/");
        assert_eq!(items.len(), MAX_ITEMS);
        assert!(
            items
                .iter()
                .all(|i| matches!(i.source, CompletionSource::Builtin))
        );
    }

    #[test]
    fn prefix_filters() {
        let app = App::new("0.1.0");
        let items = candidates(&app, "/cl");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].value, "clear");
    }

    #[test]
    fn skills_appear_alongside_builtins() {
        let mut app = App::new("0.1.0");
        app.skills.push("pdf-processing".into());
        app.skills.push("echo-test".into());
        let items = candidates(&app, "/");
        let has_skill = items.iter().any(|i| i.source == CompletionSource::Skill);
        assert!(has_skill);
    }

    #[test]
    fn builtin_wins_on_collision() {
        let mut app = App::new("0.1.0");
        app.skills.push("clear".into());
        let items = candidates(&app, "/cl");
        assert_eq!(items.iter().filter(|i| i.value == "clear").count(), 1);
        assert_eq!(
            items[0].source,
            CompletionSource::Builtin,
            "built-in must win on name collision"
        );
    }

    #[test]
    fn common_prefix_of_candidates() {
        let names = vec!["config", "creds", "clear"];
        assert_eq!(common_prefix(names), "c");
        let more = vec!["clear", "clear"];
        assert_eq!(common_prefix(more), "clear");
    }
}
