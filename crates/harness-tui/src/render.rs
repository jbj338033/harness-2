use crate::app::{App, ApprovalRequest, Entry, EntryKind, Overlay, color_enabled};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

const PRIMARY: Color = Color::Rgb(183, 167, 235);
const PRIMARY_DIM: Color = Color::Rgb(140, 125, 185);

const PROMPT: &str = "▸ ";

fn primary() -> Style {
    if color_enabled() {
        Style::default().fg(PRIMARY)
    } else {
        Style::default()
    }
}

fn primary_dim() -> Style {
    if color_enabled() {
        Style::default().fg(PRIMARY_DIM)
    } else {
        Style::default().add_modifier(Modifier::DIM)
    }
}

pub fn draw(f: &mut Frame, app: &App) {
    let area = f.area();
    let input_rows = input_wrap_rows(&app.input, area.width);
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(input_rows),
            Constraint::Length(1),
        ])
        .split(area);

    draw_log(f, layout[0], app);
    draw_input(f, layout[1], app);
    draw_status(f, layout[2], app);
    draw_completions(f, layout[1], app);
    draw_approval(f, area, app);
    draw_overlay(f, area, app);
}

fn draw_completions(f: &mut Frame, input_area: Rect, app: &App) {
    let items = crate::completion::candidates(app, &app.input);
    if items.is_empty() {
        return;
    }
    let popup_height = u16::try_from(items.len()).unwrap_or(1);
    if input_area.y < popup_height {
        return;
    }
    let popup_area = Rect {
        x: input_area.x,
        y: input_area.y - popup_height,
        width: input_area.width,
        height: popup_height,
    };
    let value_cols = items
        .iter()
        .map(|i| Span::raw(i.value.as_str().to_string()).width())
        .max()
        .unwrap_or(0);
    let mut lines: Vec<Line> = Vec::with_capacity(items.len());
    for (i, item) in items.iter().enumerate() {
        let highlight = i == 0;
        let value_style = if highlight {
            primary().add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        let glyph_style = if color_enabled() {
            match item.source {
                crate::completion::CompletionSource::Builtin => primary(),
                crate::completion::CompletionSource::Skill => Style::default().fg(Color::Magenta),
            }
        } else {
            Style::default()
        };
        let this_cols = Span::raw(item.value.as_str().to_string()).width();
        let pad = " ".repeat(value_cols.saturating_sub(this_cols));
        lines.push(Line::from(vec![
            Span::styled(format!("{} ", item.source.glyph()), glyph_style),
            Span::styled(format!("/{}{pad}", item.value), value_style),
            Span::raw("  "),
            Span::styled(
                item.description.clone(),
                Style::default().add_modifier(Modifier::DIM),
            ),
        ]));
    }
    let para = Paragraph::new(lines);
    f.render_widget(Clear, popup_area);
    f.render_widget(para, popup_area);
}

fn input_wrap_rows(input: &str, width: u16) -> u16 {
    let w = usize::from(width).max(1);
    let prefix = Span::raw(PROMPT).width();
    let cont = 2usize;
    let mut rows = 0usize;
    for (i, line) in input.split('\n').enumerate() {
        let lead = if i == 0 { prefix } else { cont };
        let content = Span::raw(line.to_string()).width();
        let total = (lead + content).max(1);
        rows += total.div_ceil(w);
    }
    u16::try_from(rows.clamp(1, 6)).unwrap_or(1)
}

fn draw_log(f: &mut Frame, area: Rect, app: &App) {
    let tail = &app.entries[app.committed_count..];
    let mut lines: Vec<Line> = Vec::with_capacity(tail.len() * 2);
    for (i, entry) in tail.iter().enumerate() {
        if i > 0 || app.committed_count > 0 {
            lines.push(Line::from(""));
        }
        for line in entry_to_lines(entry) {
            lines.push(line);
        }
    }
    let viewport_rows = usize::from(area.height);
    let scroll = u16::try_from(lines.len().saturating_sub(viewport_rows)).unwrap_or(u16::MAX);
    let para = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .scroll((scroll, 0));
    f.render_widget(para, area);
}

fn prefix_glyph(
    body: Vec<Line<'static>>,
    glyph: &'static str,
    glyph_style: Style,
) -> Vec<Line<'static>> {
    if body.is_empty() {
        return vec![Line::from(Span::styled(glyph.to_string(), glyph_style))];
    }
    body.into_iter()
        .enumerate()
        .map(|(i, line)| {
            let mut spans: Vec<Span<'static>> = Vec::with_capacity(line.spans.len() + 1);
            if i == 0 {
                spans.push(Span::styled(glyph.to_string(), glyph_style));
            } else {
                spans.push(Span::raw("  ".to_string()));
            }
            spans.extend(line.spans);
            Line::from(spans)
        })
        .collect()
}

pub(crate) fn entry_to_lines(e: &Entry) -> Vec<Line<'static>> {
    let (glyph, glyph_style) = glyph_for(&e.kind);
    match &e.kind {
        EntryKind::Banner => {
            let brand = primary().add_modifier(Modifier::BOLD);
            let version = Style::default().add_modifier(Modifier::DIM);
            vec![
                Line::from(vec![
                    Span::styled("◆ ", primary()),
                    Span::styled("harness", brand),
                    Span::styled(format!("  v{}", e.text), version),
                ]),
                Line::from(Span::styled(
                    "  /help for commands · /quit to exit",
                    Style::default().add_modifier(Modifier::DIM),
                )),
            ]
        }
        EntryKind::Assistant => prefix_glyph(crate::markdown::render(&e.text), glyph, glyph_style),
        _ => e
            .text
            .lines()
            .enumerate()
            .map(|(i, text)| {
                if i == 0 {
                    Line::from(vec![
                        Span::styled(glyph.to_string(), glyph_style),
                        Span::raw(text.to_string()),
                    ])
                } else {
                    Line::from(vec![
                        Span::raw("  ".to_string()),
                        Span::raw(text.to_string()),
                    ])
                }
            })
            .collect(),
    }
}

fn glyph_for(kind: &EntryKind) -> (&'static str, Style) {
    let bold = Style::default().add_modifier(Modifier::BOLD);
    let dim = Style::default().add_modifier(Modifier::DIM);
    match kind {
        EntryKind::User => ("▸ ", primary().add_modifier(Modifier::BOLD)),
        EntryKind::Assistant => ("● ", primary()),
        EntryKind::Daemon => ("◂ ", dim),
        EntryKind::System => ("· ", dim),
        EntryKind::Error => (
            "✗ ",
            if color_enabled() {
                bold.fg(Color::Red)
            } else {
                bold
            },
        ),
        EntryKind::ToolCall => (
            "→ ",
            if color_enabled() {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default()
            },
        ),
        EntryKind::Banner => ("", Style::default()),
    }
}

fn draw_input(f: &mut Frame, area: Rect, app: &App) {
    let prefix_cols = u16::try_from(Span::raw(PROMPT).width()).unwrap_or(2);
    let mut lines: Vec<Line> = Vec::new();
    for (i, text) in app.input.split('\n').enumerate() {
        if i == 0 {
            lines.push(Line::from(vec![
                Span::styled(PROMPT, primary().add_modifier(Modifier::BOLD)),
                Span::raw(text.to_string()),
            ]));
        } else {
            lines.push(Line::from(vec![
                Span::raw("  ".to_string()),
                Span::raw(text.to_string()),
            ]));
        }
    }
    let para = Paragraph::new(lines).wrap(Wrap { trim: false });
    f.render_widget(para, area);

    let w = area.width.max(1);
    let mut row_offset: u16 = 0;
    let pieces: Vec<&str> = app.input.split('\n').collect();
    let last_idx = pieces.len().saturating_sub(1);
    for (i, piece) in pieces.iter().enumerate() {
        let lead = if i == 0 { prefix_cols } else { 2 };
        let content = u16::try_from(Span::raw((*piece).to_string()).width()).unwrap_or(0);
        let cols = lead.saturating_add(content);
        if i == last_idx {
            let final_row = cols / w;
            let final_col = cols % w;
            let cursor_x = area
                .x
                .saturating_add(final_col)
                .min(area.x.saturating_add(area.width.saturating_sub(1)));
            let cursor_y = area.y.saturating_add(row_offset).saturating_add(final_row);
            f.set_cursor_position((cursor_x, cursor_y));
            return;
        }
        let wrap = cols.max(1).div_ceil(w);
        row_offset = row_offset.saturating_add(wrap);
    }
    f.set_cursor_position((area.x.saturating_add(prefix_cols), area.y));
}

fn draw_status(f: &mut Frame, area: Rect, app: &App) {
    let dim = Style::default().add_modifier(Modifier::DIM);
    let sep = Span::styled("  │  ", dim);

    let (dot, dot_style, label, label_style) = if !app.connected {
        ("○", dim, "offline", dim)
    } else if app.turn_running {
        (
            "●",
            primary().add_modifier(Modifier::BOLD),
            "thinking…",
            primary(),
        )
    } else {
        ("●", primary(), "online", dim)
    };

    let mut spans = vec![
        Span::raw(" "),
        Span::styled(dot, dot_style),
        Span::raw(" "),
        Span::styled(label, label_style),
        sep.clone(),
    ];

    if let Some(sid) = app.session_id.as_deref() {
        spans.push(Span::styled("session", dim));
        spans.push(Span::raw(" "));
        spans.push(Span::styled(format!("·{}", short_id(sid)), primary_dim()));
    } else {
        spans.push(Span::styled("no session", dim));
    }

    if let Some(model) = app.model.as_deref() {
        spans.push(sep);
        spans.push(Span::styled(model.to_string(), dim));
    }

    let para = Paragraph::new(Line::from(spans));
    f.render_widget(para, area);
}

fn short_id(id: &str) -> String {
    let tail = id.len().saturating_sub(6);
    format!("…{}", &id[tail..])
}

fn draw_overlay(f: &mut Frame, area: Rect, app: &App) {
    let title;
    let body: Vec<Line>;
    match &app.overlay {
        Overlay::None => return,
        Overlay::Help => {
            title = "help [esc to close]";
            body = help_body();
        }
        Overlay::Sessions => {
            title = "sessions [esc to close]";
            body = app
                .session_list
                .iter()
                .map(|s| {
                    let t = s.title.as_deref().unwrap_or("(untitled)");
                    Line::from(format!("{} │ {t} │ {}", short_id(&s.id), s.cwd))
                })
                .collect();
        }
        Overlay::Config => {
            title = "config [esc to close]";
            body = app
                .config_list
                .iter()
                .map(|(k, v)| Line::from(format!("{k} = {v}")))
                .collect();
        }
        Overlay::Devices => {
            title = "devices [esc to close]";
            body = app
                .device_list
                .iter()
                .map(|d| Line::from(format!("{} │ {} │ {:?}", d.id, d.name, d.last_seen_at)))
                .collect();
        }
        Overlay::Agents => {
            title = "agents [esc to close]";
            body = vec![
                Line::from(format!(
                    "session: {}",
                    app.session_id.as_deref().unwrap_or("—")
                )),
                Line::from(format!(
                    "root agent: {}",
                    app.agent_id.as_deref().unwrap_or("—")
                )),
                Line::from(format!("model: {}", app.model.as_deref().unwrap_or("—"))),
            ];
        }
    }

    let popup = centered(area, 70, 60);
    f.render_widget(Clear, popup);
    let title_span = Span::styled(title, primary().add_modifier(Modifier::BOLD));
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(primary_dim())
        .title(title_span);
    let para = Paragraph::new(body).block(block).wrap(Wrap { trim: false });
    f.render_widget(para, popup);
}

fn draw_approval(f: &mut Frame, area: Rect, app: &App) {
    let Some(ApprovalRequest {
        description,
        pattern,
        ..
    }) = &app.pending_approval
    else {
        return;
    };
    let popup = centered(area, 60, 30);
    f.render_widget(Clear, popup);
    let text = vec![
        Line::from(vec![
            Span::styled("● ", primary()),
            Span::styled("실행 요청", primary().add_modifier(Modifier::BOLD)),
        ]),
        Line::from(description.clone()),
        Line::from(""),
        Line::from(vec![
            Span::styled("pattern  ", Style::default().add_modifier(Modifier::DIM)),
            Span::raw(pattern.clone()),
        ]),
        Line::from(""),
        Line::from("[y] allow   [n] deny   [a] allow this pattern for session"),
    ];
    let title = Span::styled("approval required", primary().add_modifier(Modifier::BOLD));
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(primary_dim())
        .title(title);
    let para = Paragraph::new(text).block(block).wrap(Wrap { trim: false });
    f.render_widget(para, popup);
}

fn centered(area: Rect, width_pct: u16, height_pct: u16) -> Rect {
    let v = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - height_pct) / 2),
            Constraint::Percentage(height_pct),
            Constraint::Percentage((100 - height_pct) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - width_pct) / 2),
            Constraint::Percentage(width_pct),
            Constraint::Percentage((100 - width_pct) / 2),
        ])
        .split(v[1])[1]
}

fn help_body() -> Vec<Line<'static>> {
    vec![
        Line::from("keys:"),
        Line::from("  Enter       send input (/command or chat message)"),
        Line::from("  Shift+Enter newline in input"),
        Line::from("  Ctrl+C      cancel current turn (or quit if idle)"),
        Line::from("  Ctrl+D      quit"),
        Line::from("  Ctrl+L      clear timeline"),
        Line::from("  Ctrl+P/N    previous / next input history"),
        Line::from("  Ctrl+A      agents overlay"),
        Line::from("  Ctrl+H      sessions overlay"),
        Line::from("  Ctrl+,      config overlay"),
        Line::from("  Ctrl+/      this help"),
        Line::from("commands: /help for the full list"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    #[test]
    fn draws_without_panic() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let app = App::new("0.1.0");
        terminal.draw(|f| draw(f, &app)).unwrap();
    }

    #[test]
    fn draws_overlay_without_panic() {
        let backend = TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::new("0.1.0");
        app.overlay = Overlay::Help;
        terminal.draw(|f| draw(f, &app)).unwrap();
    }

    #[test]
    fn draws_approval_without_panic() {
        let backend = TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::new("0.1.0");
        app.pending_approval = Some(crate::app::ApprovalRequest {
            id: "a".into(),
            description: "rm".into(),
            pattern: "rm -rf".into(),
        });
        terminal.draw(|f| draw(f, &app)).unwrap();
    }

    #[test]
    fn hangul_input_soft_wraps_past_viewport_width() {
        let rows = input_wrap_rows("안녕하세요반갑습니다", 10);
        assert!(rows >= 2, "expected soft-wrap across rows, got {rows}");
    }

    #[test]
    fn ascii_input_stays_one_row_when_it_fits() {
        let rows = input_wrap_rows("hi", 80);
        assert_eq!(rows, 1);
    }

    #[test]
    fn draws_with_hangul_input_without_panic() {
        let backend = TestBackend::new(40, 12);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::new("0.1.0");
        app.input = "안녕하세요 반갑습니다".into();
        terminal.draw(|f| draw(f, &app)).unwrap();
    }
}
