use crate::app::color_enabled;
use crate::highlight;
use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

const PRIMARY: Color = Color::Rgb(183, 167, 235);
const CODE_FG: Color = Color::Rgb(220, 200, 240);

fn primary() -> Style {
    if color_enabled() {
        Style::default().fg(PRIMARY)
    } else {
        Style::default()
    }
}

fn inline_code() -> Style {
    if color_enabled() {
        Style::default().fg(CODE_FG).add_modifier(Modifier::BOLD)
    } else {
        Style::default().add_modifier(Modifier::BOLD)
    }
}

pub fn render(md: &str) -> Vec<Line<'static>> {
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    let parser = Parser::new_ext(md, opts);
    let mut state = State::default();
    for ev in parser {
        state.handle(ev);
    }
    state.finish()
}

#[derive(Default)]
struct State {
    lines: Vec<Line<'static>>,
    spans: Vec<Span<'static>>,
    style_stack: Vec<Style>,
    list_stack: Vec<Option<u64>>,
    list_idx: Vec<u64>,
    in_code: bool,
    code_lang: String,
    code_buf: String,
}

impl State {
    fn top_style(&self) -> Style {
        self.style_stack.last().copied().unwrap_or_default()
    }

    fn push_text(&mut self, t: String) {
        let s = self.top_style();
        self.spans.push(Span::styled(t, s));
    }

    fn flush(&mut self) {
        if !self.spans.is_empty() {
            let spans = std::mem::take(&mut self.spans);
            self.lines.push(Line::from(spans));
        }
    }

    fn emit_code_block(&mut self) {
        let code = std::mem::take(&mut self.code_buf);
        let lang = std::mem::take(&mut self.code_lang);
        let trimmed = code.trim_end_matches('\n');
        for l in highlight::highlight(trimmed, &lang) {
            self.lines.push(l);
        }
    }

    fn finish(mut self) -> Vec<Line<'static>> {
        if self.in_code && !self.code_buf.is_empty() {
            self.emit_code_block();
        }
        self.flush();
        self.lines
    }

    fn handle(&mut self, ev: Event<'_>) {
        match ev {
            Event::Start(tag) => self.handle_start(tag),
            Event::End(tag) => self.handle_end(tag),
            Event::HardBreak => self.flush(),
            Event::Code(c) => self
                .spans
                .push(Span::styled(c.into_string(), inline_code())),
            Event::Text(t) => {
                if self.in_code {
                    self.code_buf.push_str(&t);
                } else {
                    self.push_text(t.into_string());
                }
            }
            Event::SoftBreak => self.push_text(" ".to_string()),
            Event::Rule => {
                self.flush();
                self.lines.push(Line::from(Span::styled(
                    "─".repeat(40),
                    Style::default().add_modifier(Modifier::DIM),
                )));
            }
            _ => {}
        }
    }

    fn handle_start(&mut self, tag: Tag<'_>) {
        match tag {
            Tag::Heading { level, .. } => {
                self.flush();
                let hashes = "#".repeat(heading_depth(level).into());
                let sty = primary().add_modifier(Modifier::BOLD);
                self.spans.push(Span::styled(format!("{hashes} "), sty));
                self.style_stack.push(sty);
            }
            Tag::Strong => {
                let s = self.top_style().add_modifier(Modifier::BOLD);
                self.style_stack.push(s);
            }
            Tag::Emphasis => {
                let s = self.top_style().add_modifier(Modifier::ITALIC);
                self.style_stack.push(s);
            }
            Tag::Strikethrough => {
                let s = self.top_style().add_modifier(Modifier::CROSSED_OUT);
                self.style_stack.push(s);
            }
            Tag::CodeBlock(kind) => {
                self.flush();
                self.in_code = true;
                self.code_lang = match kind {
                    CodeBlockKind::Fenced(l) => l.to_string(),
                    CodeBlockKind::Indented => String::new(),
                };
                self.code_buf.clear();
            }
            Tag::List(start) => {
                self.list_stack.push(start);
                self.list_idx.push(start.unwrap_or(1));
            }
            Tag::Item => {
                let depth = self.list_stack.len().saturating_sub(1);
                let indent = "  ".repeat(depth);
                let marker = if matches!(self.list_stack.last(), Some(Some(_))) {
                    let i = self.list_idx.last().copied().unwrap_or(1);
                    format!("{i}. ")
                } else {
                    "• ".to_string()
                };
                self.spans
                    .push(Span::styled(format!("{indent}{marker}"), primary()));
            }
            Tag::Link { .. } => {
                let s = self.top_style().add_modifier(Modifier::UNDERLINED);
                self.style_stack.push(s);
            }
            Tag::BlockQuote(_) => {
                self.flush();
                self.spans.push(Span::styled(
                    "│ ".to_string(),
                    Style::default().add_modifier(Modifier::DIM),
                ));
                let s = self.top_style().add_modifier(Modifier::DIM);
                self.style_stack.push(s);
            }
            _ => {}
        }
    }

    fn handle_end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Paragraph => self.flush(),
            TagEnd::Strong | TagEnd::Emphasis | TagEnd::Strikethrough | TagEnd::Link => {
                self.style_stack.pop();
            }
            TagEnd::Heading(_) | TagEnd::BlockQuote(_) => {
                self.style_stack.pop();
                self.flush();
            }
            TagEnd::CodeBlock => {
                self.emit_code_block();
                self.in_code = false;
            }
            TagEnd::List(_) => {
                self.list_stack.pop();
                self.list_idx.pop();
            }
            TagEnd::Item => {
                self.flush();
                if matches!(self.list_stack.last(), Some(Some(_)))
                    && let Some(i) = self.list_idx.last_mut()
                {
                    *i += 1;
                }
            }
            _ => {}
        }
    }
}

fn heading_depth(l: HeadingLevel) -> u8 {
    match l {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(lines: &[Line<'_>]) -> Vec<String> {
        lines
            .iter()
            .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
            .collect()
    }

    #[test]
    fn renders_heading_and_paragraph() {
        let out = render("# Title\n\nbody text");
        let flat = texts(&out);
        assert!(flat.iter().any(|l| l.contains("# Title")));
        assert!(flat.iter().any(|l| l.contains("body text")));
    }

    #[test]
    fn renders_bullet_list_with_markers() {
        let out = render("- one\n- two");
        let flat = texts(&out);
        assert!(flat.iter().any(|l| l.contains("• one")));
        assert!(flat.iter().any(|l| l.contains("• two")));
    }

    #[test]
    fn renders_ordered_list_starting_from_one() {
        let out = render("1. first\n2. second");
        let flat = texts(&out);
        assert!(flat.iter().any(|l| l.contains("1. first")));
        assert!(flat.iter().any(|l| l.contains("2. second")));
    }

    #[test]
    fn renders_fenced_code_block() {
        let out = render("```rust\nfn main() {}\n```");
        let flat = texts(&out).join("\n");
        assert!(flat.contains("fn main"));
    }

    #[test]
    fn unterminated_fenced_code_still_flushes_on_finish() {
        let out = render("```rust\nfn partial() {");
        let flat = texts(&out).join("\n");
        assert!(flat.contains("fn partial"));
    }

    #[test]
    fn inline_code_is_preserved() {
        let out = render("use `foo::bar` for this");
        let flat = texts(&out).join("");
        assert!(flat.contains("foo::bar"));
    }

    #[test]
    fn renders_korean_text_without_mangling() {
        let out = render("**안녕** 하세요");
        let flat = texts(&out).join("");
        assert!(flat.contains("안녕"));
        assert!(flat.contains("하세요"));
    }
}
