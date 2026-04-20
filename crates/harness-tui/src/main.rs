mod app;
mod chat;
mod client;
mod commands;
mod completion;
mod highlight;
mod input;
mod markdown;
mod render;

use anyhow::{Context, Result};
use app::App;
use crossterm::{
    event::{self, Event},
    terminal::{disable_raw_mode, enable_raw_mode},
};
use harness_proto::{Request, SUPPORTED_VERSIONS};
use ratatui::text::Line;
use ratatui::widgets::{Paragraph, Widget, Wrap};
use ratatui::{Terminal, TerminalOptions, Viewport, backend::CrosstermBackend};
use serde_json::json;
use std::path::PathBuf;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::net::UnixStream;
use tokio::net::unix::OwnedWriteHalf;
use tokio::sync::mpsc;

const VERSION: &str = env!("CARGO_PKG_VERSION");

pub enum AppEvent {
    Key(crossterm::event::KeyEvent),
    DaemonLine(String),
    Tick,
    Quit,
}

fn inline_viewport_height() -> u16 {
    use crossterm::terminal::size;
    let (_, rows) = size().unwrap_or((80, 24));
    let budget = rows.saturating_sub(2);
    budget.clamp(6, 12)
}

fn socket_path() -> PathBuf {
    if let Ok(p) = std::env::var("HARNESS_DATA_DIR") {
        return PathBuf::from(p).join("harness.sock");
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".harness").join("harness.sock")
}

#[derive(Debug, Clone)]
enum StartupAction {
    New,
    Continue,
    Resume(Option<String>),
}

enum ArgOutcome {
    Tui(StartupAction),
    Subcommand(Vec<String>),
}

fn parse_args() -> ArgOutcome {
    if let Some(first) = std::env::args().nth(1)
        && harness_cli::is_subcommand(&first)
    {
        let cli_args: Vec<String> = std::env::args().skip(1).collect();
        return ArgOutcome::Subcommand(cli_args);
    }

    let mut args = std::env::args().skip(1).peekable();
    let mut action = StartupAction::New;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                print_usage();
                std::process::exit(0);
            }
            "-V" | "--version" => {
                println!("harness {VERSION}");
                std::process::exit(0);
            }
            "-c" | "--continue" => action = StartupAction::Continue,
            "-r" | "--resume" => {
                let id = args
                    .peek()
                    .filter(|a| !a.starts_with('-'))
                    .cloned()
                    .inspect(|_v| {
                        args.next();
                    });
                action = StartupAction::Resume(id);
            }
            other => {
                eprintln!("harness: unknown argument `{other}`");
                print_usage();
                std::process::exit(2);
            }
        }
    }
    ArgOutcome::Tui(action)
}

fn print_usage() {
    println!(
        "harness {VERSION}\n\
         \n\
         Usage:\n\
         \x20\x20harness                      open the TUI\n\
         \x20\x20harness [SESSION-FLAG]       open the TUI resuming a session\n\
         \x20\x20harness <SUBCOMMAND> [...]   run a CLI subcommand\n\
         \n\
         Session flags (TUI):\n\
         \x20\x20-c, --continue         resume cwd's latest session\n\
         \x20\x20-r, --resume [ID]      resume session ID (picker when omitted)\n\
         \n\
         Subcommands:\n\
         \x20\x20auth login|add|list|remove  manage provider credentials\n\
         \x20\x20model list|use|current      inspect or set the default model\n\
         \x20\x20skill list|info             browse discovered Agent Skills\n\
         \x20\x20mcp list|add|remove         register MCP servers\n\
         \x20\x20config get|set|unset|list   manage daemon config\n\
         \x20\x20device list|revoke          manage paired devices\n\
         \x20\x20pair / connect              pair devices\n\
         \x20\x20doctor                      end-to-end health check\n\
         \x20\x20setup                       interactive first-run setup\n\
         \x20\x20status                      daemon + device summary\n\
         \n\
         Run `harness <subcommand> --help` for sub-verb details."
    );
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let startup = match parse_args() {
        ArgOutcome::Tui(s) => s,
        ArgOutcome::Subcommand(args) => return harness_cli::run(args).await,
    };

    if std::env::var_os("NO_COLOR").is_some() {
        app::disable_color();
    }

    enable_raw_mode().context("enable raw mode")?;
    let stdout = std::io::stdout();
    let backend = CrosstermBackend::new(stdout);
    let viewport_height = inline_viewport_height();
    let mut terminal = Terminal::with_options(
        backend,
        TerminalOptions {
            viewport: Viewport::Inline(viewport_height),
        },
    )
    .context("create terminal")?;

    let sock = socket_path();
    let mut app = App::new(VERSION);
    let (writer, reader_task) = match UnixStream::connect(&sock).await {
        Ok(stream) => {
            let (r, mut w) = stream.into_split();
            let (line_tx, line_rx) = mpsc::channel::<String>(256);
            let reader_task = tokio::spawn(client::reader_task(r, line_tx));
            app.mark_connected(&sock.display().to_string());
            if let Err(e) = send_handshake(&mut app, &mut w, &startup).await {
                app.push_error(format!("handshake failed: {e}"));
            }
            (Some((w, line_rx)), Some(reader_task))
        }
        Err(e) => {
            app.push_error(format!(
                "could not connect to daemon at {}: {e}\nstart `harnessd` or set HARNESS_DATA_DIR",
                sock.display()
            ));
            (None, None)
        }
    };

    let result = run_ui(&mut terminal, &mut app, writer, &startup).await;

    disable_raw_mode().ok();
    terminal.clear().ok();
    terminal.show_cursor().ok();

    if let Some(task) = reader_task {
        task.abort();
    }
    result
}

fn wrapped_height(lines: &[Line<'_>], width: u16) -> u16 {
    let w = usize::from(width.max(1));
    let total: usize = lines
        .iter()
        .map(|l| {
            let cw = l.width().max(1);
            cw.div_ceil(w)
        })
        .sum();
    u16::try_from(total.max(1)).unwrap_or(u16::MAX)
}

fn commit_to_scrollback(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    app: &mut App,
) -> Result<()> {
    let width = terminal.size()?.width;
    let last_is_live = !app.streams.is_empty();
    let end = if last_is_live {
        app.entries.len().saturating_sub(1)
    } else {
        app.entries.len()
    };

    while app.committed_count < end {
        let entry = app.entries[app.committed_count].clone();
        let body = render::entry_to_lines(&entry);
        let mut lines: Vec<Line<'static>> = Vec::with_capacity(body.len() + 1);
        if app.committed_count > 0 {
            lines.push(Line::from(""));
        }
        lines.extend(body);
        let height = wrapped_height(&lines, width);
        let para = Paragraph::new(lines).wrap(Wrap { trim: false });
        terminal.insert_before(height, |buf| {
            para.render(buf.area, buf);
        })?;
        app.committed_count += 1;
    }
    Ok(())
}

type Writer = (tokio::net::unix::OwnedWriteHalf, mpsc::Receiver<String>);

async fn run_ui(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    app: &mut App,
    mut writer: Option<Writer>,
    startup: &StartupAction,
) -> Result<()> {
    let (evt_tx, mut evt_rx) = mpsc::channel::<AppEvent>(256);
    spawn_input_thread(evt_tx.clone());

    let sock = socket_path();
    let mut reconnect_backoff = Duration::from_secs(1);
    let mut next_reconnect: Option<tokio::time::Instant> = None;

    loop {
        if drain_daemon_lines(&mut writer, &evt_tx).await {
            app.mark_disconnected(reconnect_backoff);
            writer = None;
            next_reconnect = Some(tokio::time::Instant::now() + reconnect_backoff);
        }

        if writer.is_none() {
            try_reconnect(
                app,
                &mut writer,
                &sock,
                startup,
                &mut reconnect_backoff,
                &mut next_reconnect,
            )
            .await;
        }

        flush_pending_writes(app, &mut writer).await;

        commit_to_scrollback(terminal, app)?;
        terminal.draw(|f| render::draw(f, app))?;

        let event = match tokio::time::timeout(Duration::from_millis(100), evt_rx.recv()).await {
            Ok(Some(e)) => e,
            Ok(None) => AppEvent::Quit,
            Err(_) => AppEvent::Tick,
        };

        match event {
            AppEvent::Quit => break,
            AppEvent::Tick => {}
            AppEvent::Key(k) => {
                if input::handle_key(app, writer.as_mut(), k).await? {
                    break;
                }
            }
            AppEvent::DaemonLine(line) => client::handle_daemon_line(app, &line),
        }
    }
    Ok(())
}

fn spawn_input_thread(tx: mpsc::Sender<AppEvent>) {
    std::thread::spawn(move || {
        loop {
            let Ok(has) = event::poll(Duration::from_millis(50)) else {
                continue;
            };
            if !has {
                tx.blocking_send(AppEvent::Tick).ok();
                continue;
            }
            match event::read() {
                Ok(Event::Key(k)) if tx.blocking_send(AppEvent::Key(k)).is_err() => {
                    return;
                }
                Ok(Event::Resize(..)) => {
                    tx.blocking_send(AppEvent::Tick).ok();
                }
                _ => {}
            }
        }
    });
}

async fn drain_daemon_lines(writer: &mut Option<Writer>, evt_tx: &mpsc::Sender<AppEvent>) -> bool {
    let Some((_, rx)) = writer.as_mut() else {
        return false;
    };
    loop {
        match rx.try_recv() {
            Ok(line) => {
                evt_tx.send(AppEvent::DaemonLine(line)).await.ok();
            }
            Err(tokio::sync::mpsc::error::TryRecvError::Empty) => return false,
            Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => return true,
        }
    }
}

async fn try_reconnect(
    app: &mut App,
    writer: &mut Option<Writer>,
    sock: &std::path::Path,
    startup: &StartupAction,
    backoff: &mut Duration,
    next_reconnect: &mut Option<tokio::time::Instant>,
) {
    if next_reconnect.is_some_and(|t| tokio::time::Instant::now() < t) {
        return;
    }
    let Ok(stream) = UnixStream::connect(sock).await else {
        *backoff = (*backoff * 2).min(Duration::from_secs(30));
        *next_reconnect = Some(tokio::time::Instant::now() + *backoff);
        app.mark_offline(*backoff);
        return;
    };
    let (r, mut w) = stream.into_split();
    let (tx, rx) = mpsc::channel::<String>(256);
    tokio::spawn(client::reader_task(r, tx));
    app.mark_connected(&sock.display().to_string());
    app.protocol_version = None;
    if let Err(e) = send_handshake(app, &mut w, startup).await {
        app.push_error(format!("handshake failed: {e}"));
    }
    *writer = Some((w, rx));
    *backoff = Duration::from_secs(1);
    *next_reconnect = None;
}

async fn flush_pending_writes(app: &mut App, writer: &mut Option<Writer>) {
    if app.pending_writes.is_empty() {
        return;
    }
    let Some((w, _)) = writer.as_mut() else {
        return;
    };
    let queue = std::mem::take(&mut app.pending_writes);
    for pw in queue {
        let Some(req) = build_pending_request(app, pw) else {
            continue;
        };
        if let Err(e) = write_request(w, &req).await {
            app.push_error(format!("pending-write failed: {e}"));
        }
    }
}

fn build_pending_request(app: &mut App, pw: app::PendingWrite) -> Option<Request> {
    let id = app.next_id();
    Some(match pw {
        app::PendingWrite::Resume(sid) => {
            Request::new(id, "v1.session.resume", Some(json!({ "session_id": sid })))
        }
        app::PendingWrite::CreateForCwd(cwd) => {
            Request::new(id, "v1.session.create", Some(json!({ "cwd": cwd })))
        }
        app::PendingWrite::Chat(message) => {
            let Some(sid) = app.session_id.clone() else {
                app.push_error("queued message dropped: session id missing at flush");
                return None;
            };
            let mut params = json!({ "session_id": sid, "message": message });
            if let Some(m) = app.model.as_ref() {
                params["model"] = serde_json::Value::String(m.clone());
            }
            Request::new(id, "v1.chat.send", Some(params))
        }
    })
}

async fn send_handshake(
    app: &mut App,
    writer: &mut OwnedWriteHalf,
    startup: &StartupAction,
) -> Result<()> {
    let ping_id = app.next_id();
    let ping = Request::new(ping_id, "ping", None);
    write_request(writer, &ping).await?;

    let neg_id = app.next_id();
    let neg = Request::new(
        neg_id,
        "negotiate",
        Some(json!({ "client_versions": SUPPORTED_VERSIONS })),
    );
    write_request(writer, &neg).await?;

    let list_id = app.next_id();
    let list = Request::new(list_id, "v1.skill.list", None);
    write_request(writer, &list).await?;

    let creds_id = app.next_id();
    let creds = Request::new(creds_id, "v1.auth.credentials.list", None);
    write_request(writer, &creds).await?;

    if app.session_id.is_some() {
        return Ok(());
    }
    let cwd =
        std::env::current_dir().map_or_else(|_| ".".into(), |p| p.to_string_lossy().into_owned());
    match startup {
        StartupAction::New => {
            let id = app.next_id();
            let req = Request::new(id, "v1.session.create", Some(json!({ "cwd": cwd })));
            write_request(writer, &req).await?;
        }
        StartupAction::Resume(Some(sid)) => {
            let id = app.next_id();
            let req = Request::new(id, "v1.session.resume", Some(json!({ "session_id": sid })));
            write_request(writer, &req).await?;
        }
        StartupAction::Resume(None) => {
            app.overlay = app::Overlay::Sessions;
            let id = app.next_id();
            let req = Request::new(id, "v1.session.list", None);
            write_request(writer, &req).await?;
            app.push_system("pick a session with /resume <id>, or /clear for a new one");
        }
        StartupAction::Continue => {
            app.continue_for_cwd = Some(cwd.clone());
            let id = app.next_id();
            let req = Request::new(id, "v1.session.list", None);
            write_request(writer, &req).await?;
        }
    }
    Ok(())
}

async fn write_request(w: &mut OwnedWriteHalf, req: &Request) -> Result<()> {
    let text = serde_json::to_string(req)?;
    w.write_all(text.as_bytes()).await?;
    w.write_all(b"\n").await?;
    Ok(())
}
