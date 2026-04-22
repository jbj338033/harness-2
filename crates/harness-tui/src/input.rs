use crate::app::{App, ApprovalRequest, CTRL_C_DOUBLE_PRESS_WINDOW, Overlay};
use crate::chat;
use crate::commands;
use crate::completion;
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use harness_proto::Request;
use serde_json::json;
use std::time::Instant;
use tokio::io::AsyncWriteExt;
use tokio::net::unix::OwnedWriteHalf;
use tokio::sync::mpsc;

pub async fn handle_key(
    app: &mut App,
    mut writer: Option<&mut (OwnedWriteHalf, mpsc::Receiver<String>)>,
    k: KeyEvent,
) -> Result<bool> {
    if let Some(pending) = app.pending_approval.clone() {
        return handle_approval_key(app, writer.map(|(w, _)| w), pending, k).await;
    }

    if !matches!(app.overlay, Overlay::None) && matches!(k.code, KeyCode::Esc) {
        app.overlay = Overlay::None;
        return Ok(false);
    }

    if let Some(quit) = handle_control(app, writer.as_deref_mut(), k).await? {
        return Ok(quit);
    }
    handle_edit(app, writer, k).await
}

async fn handle_control(
    app: &mut App,
    writer: Option<&mut (OwnedWriteHalf, mpsc::Receiver<String>)>,
    k: KeyEvent,
) -> Result<Option<bool>> {
    if k.modifiers != KeyModifiers::CONTROL {
        return Ok(None);
    }
    match k.code {
        KeyCode::Char('c') => Ok(Some(handle_ctrl_c(app))),
        KeyCode::Char('d') => Ok(Some(true)),
        KeyCode::Char('l') => {
            app.clear();
            Ok(Some(false))
        }
        KeyCode::Char('p') => {
            app.history_prev();
            Ok(Some(false))
        }
        KeyCode::Char('n') => {
            app.history_next();
            Ok(Some(false))
        }
        KeyCode::Char('a') => {
            app.overlay = Overlay::Agents;
            Ok(Some(false))
        }
        KeyCode::Char('h') => {
            app.overlay = Overlay::Sessions;
            if let Some((w, _)) = writer {
                let id = app.next_id();
                write_request(w, &Request::new(id, "v1.session.list", None)).await?;
            }
            Ok(Some(false))
        }
        KeyCode::Char(',') => {
            app.overlay = Overlay::Config;
            if let Some((w, _)) = writer {
                let id = app.next_id();
                write_request(w, &Request::new(id, "v1.config.list", None)).await?;
            }
            Ok(Some(false))
        }
        KeyCode::Char('/') => {
            app.overlay = Overlay::Help;
            Ok(Some(false))
        }
        _ => Ok(None),
    }
}

fn handle_ctrl_c(app: &mut App) -> bool {
    if !app.input.is_empty() {
        app.input.clear();
        app.last_ctrl_c_at = None;
        return false;
    }
    let now = Instant::now();
    let is_double = app
        .last_ctrl_c_at
        .is_some_and(|t| now.duration_since(t) <= CTRL_C_DOUBLE_PRESS_WINDOW);
    if is_double {
        return true;
    }
    app.last_ctrl_c_at = Some(now);
    app.push_system("press Ctrl+C again to exit");
    false
}

async fn handle_edit(
    app: &mut App,
    writer: Option<&mut (OwnedWriteHalf, mpsc::Receiver<String>)>,
    k: KeyEvent,
) -> Result<bool> {
    match (k.code, k.modifiers) {
        (KeyCode::Esc, _) => {
            app.input.clear();
            Ok(false)
        }
        (KeyCode::Enter, KeyModifiers::SHIFT) => {
            app.input.push('\n');
            Ok(false)
        }
        (KeyCode::Enter, _) => handle_enter(app, writer).await,
        (KeyCode::Tab, _) => {
            apply_completion(app);
            Ok(false)
        }
        (KeyCode::Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) => {
            app.input.push(c);
            Ok(false)
        }
        (KeyCode::Backspace, _) => {
            app.input.pop();
            Ok(false)
        }
        _ => Ok(false),
    }
}

async fn handle_enter(
    app: &mut App,
    writer: Option<&mut (OwnedWriteHalf, mpsc::Receiver<String>)>,
) -> Result<bool> {
    let text = app.take_input();
    if text.is_empty() {
        return Ok(false);
    }
    app.push_user(&text);
    if text.starts_with('/') {
        let act = commands::dispatch(app, writer.map(|(w, _)| w), &text).await?;
        return Ok(matches!(act, commands::Action::Quit));
    }
    chat::send_chat(app, writer.map(|(w, _)| w), text).await?;
    Ok(false)
}

fn apply_completion(app: &mut App) {
    let items = completion::candidates(app, &app.input);
    if items.is_empty() {
        return;
    }
    let values: Vec<&str> = items.iter().map(|i| i.value.as_str()).collect();
    let common = completion::common_prefix(values);
    if items.len() == 1 {
        app.input = format!("/{} ", items[0].value);
    } else if !common.is_empty() {
        app.input = format!("/{common}");
    }
}

// IMPLEMENTS: D-206
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ApprovalChoice {
    AllowOnce,
    AllowSession,
    AllowGlobal,
    DenyOnce,
    StopTurn,
}

impl ApprovalChoice {
    fn from_key(k: KeyCode) -> Option<Self> {
        match k {
            KeyCode::Char('1' | 'y' | 'Y') => Some(Self::AllowOnce),
            KeyCode::Char('2' | 's' | 'S') => Some(Self::AllowSession),
            KeyCode::Char('3' | 'a' | 'A') => Some(Self::AllowGlobal),
            KeyCode::Char('4' | 'n' | 'N') => Some(Self::DenyOnce),
            KeyCode::Char('5' | 'c' | 'C') | KeyCode::Esc => Some(Self::StopTurn),
            _ => None,
        }
    }

    fn decision(self) -> &'static str {
        match self {
            Self::AllowOnce => "allow",
            Self::AllowSession => "allow_session",
            Self::AllowGlobal => "allow_global",
            Self::DenyOnce | Self::StopTurn => "deny",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::AllowOnce => "allow once",
            Self::AllowSession => "allow session",
            Self::AllowGlobal => "allow always",
            Self::DenyOnce => "deny",
            Self::StopTurn => "stop turn",
        }
    }
}

async fn handle_approval_key(
    app: &mut App,
    writer: Option<&mut OwnedWriteHalf>,
    pending: ApprovalRequest,
    k: KeyEvent,
) -> Result<bool> {
    let Some(choice) = ApprovalChoice::from_key(k.code) else {
        return Ok(false);
    };
    app.pending_approval = None;
    let decision = choice.decision();
    let agent_id = app.agent_id.clone();
    if let Some(w) = writer {
        let id = app.next_id();
        let req = Request::new(
            id,
            "v1.approval.respond",
            Some(json!({
                "request_id": pending.id,
                "decision": decision,
                "pattern": pending.pattern,
            })),
        );
        write_request(w, &req).await?;
        if matches!(choice, ApprovalChoice::StopTurn)
            && let Some(aid) = agent_id
        {
            let id = app.next_id();
            let req = Request::new(id, "v1.chat.cancel", Some(json!({ "agent_id": aid })));
            write_request(w, &req).await?;
        }
    }
    app.push_system(format!("approval: {}", choice.label()));
    Ok(false)
}

async fn write_request(w: &mut OwnedWriteHalf, req: &Request) -> Result<()> {
    let text = serde_json::to_string(req)?;
    w.write_all(text.as_bytes()).await?;
    w.write_all(b"\n").await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

    fn key(c: KeyCode, m: KeyModifiers) -> KeyEvent {
        KeyEvent {
            code: c,
            modifiers: m,
            kind: KeyEventKind::Press,
            state: KeyEventState::empty(),
        }
    }

    #[tokio::test]
    async fn ctrl_c_first_press_clears_input() {
        let mut app = App::new("0.1.0");
        app.input = "draft".into();
        let quit = handle_key(
            &mut app,
            None,
            key(KeyCode::Char('c'), KeyModifiers::CONTROL),
        )
        .await
        .unwrap();
        assert!(
            !quit,
            "first Ctrl+C on non-empty input should clear, not quit"
        );
        assert!(app.input.is_empty());
        assert!(
            app.last_ctrl_c_at.is_none(),
            "clearing input resets the quit timer"
        );
    }

    #[tokio::test]
    async fn ctrl_c_on_empty_input_shows_hint_first() {
        let mut app = App::new("0.1.0");
        let quit = handle_key(
            &mut app,
            None,
            key(KeyCode::Char('c'), KeyModifiers::CONTROL),
        )
        .await
        .unwrap();
        assert!(!quit, "first Ctrl+C on empty input should hint, not quit");
        assert!(app.last_ctrl_c_at.is_some());
        assert!(
            app.entries
                .iter()
                .any(|e| e.text.contains("press Ctrl+C again"))
        );
    }

    #[tokio::test]
    async fn ctrl_c_double_press_within_window_quits() {
        let mut app = App::new("0.1.0");
        handle_key(
            &mut app,
            None,
            key(KeyCode::Char('c'), KeyModifiers::CONTROL),
        )
        .await
        .unwrap();
        let quit = handle_key(
            &mut app,
            None,
            key(KeyCode::Char('c'), KeyModifiers::CONTROL),
        )
        .await
        .unwrap();
        assert!(quit, "second Ctrl+C inside the window should quit");
    }

    #[tokio::test]
    async fn ctrl_c_outside_window_hints_again() {
        let mut app = App::new("0.1.0");
        handle_key(
            &mut app,
            None,
            key(KeyCode::Char('c'), KeyModifiers::CONTROL),
        )
        .await
        .unwrap();
        app.last_ctrl_c_at = Some(
            app.last_ctrl_c_at
                .unwrap()
                .checked_sub(CTRL_C_DOUBLE_PRESS_WINDOW)
                .unwrap()
                .checked_sub(std::time::Duration::from_millis(100))
                .unwrap(),
        );
        let quit = handle_key(
            &mut app,
            None,
            key(KeyCode::Char('c'), KeyModifiers::CONTROL),
        )
        .await
        .unwrap();
        assert!(
            !quit,
            "outside the window, the timer resets — should hint again"
        );
    }

    #[tokio::test]
    async fn ctrl_l_clears_timeline() {
        let mut app = App::new("0.1.0");
        app.push_user("x");
        app.push_user("y");
        handle_key(
            &mut app,
            None,
            key(KeyCode::Char('l'), KeyModifiers::CONTROL),
        )
        .await
        .unwrap();
        assert_eq!(app.entries.len(), 1);
    }

    #[tokio::test]
    async fn esc_clears_input() {
        let mut app = App::new("0.1.0");
        app.input = "draft".into();
        handle_key(&mut app, None, key(KeyCode::Esc, KeyModifiers::NONE))
            .await
            .unwrap();
        assert!(app.input.is_empty());
    }

    #[tokio::test]
    async fn shift_enter_adds_newline() {
        let mut app = App::new("0.1.0");
        handle_key(&mut app, None, key(KeyCode::Enter, KeyModifiers::SHIFT))
            .await
            .unwrap();
        assert_eq!(app.input, "\n");
    }

    #[tokio::test]
    async fn overlay_esc_closes_overlay() {
        let mut app = App::new("0.1.0");
        app.overlay = Overlay::Config;
        handle_key(&mut app, None, key(KeyCode::Esc, KeyModifiers::NONE))
            .await
            .unwrap();
        assert!(matches!(app.overlay, Overlay::None));
    }

    #[tokio::test]
    async fn approval_yes_clears_pending() {
        let mut app = App::new("0.1.0");
        app.pending_approval = Some(ApprovalRequest {
            id: "a1".into(),
            description: "rm".into(),
            pattern: "rm -rf".into(),
        });
        handle_key(&mut app, None, key(KeyCode::Char('y'), KeyModifiers::NONE))
            .await
            .unwrap();
        assert!(app.pending_approval.is_none());
    }

    #[test]
    fn approval_choice_maps_every_documented_key() {
        let cases = [
            (KeyCode::Char('1'), ApprovalChoice::AllowOnce),
            (KeyCode::Char('y'), ApprovalChoice::AllowOnce),
            (KeyCode::Char('2'), ApprovalChoice::AllowSession),
            (KeyCode::Char('s'), ApprovalChoice::AllowSession),
            (KeyCode::Char('3'), ApprovalChoice::AllowGlobal),
            (KeyCode::Char('a'), ApprovalChoice::AllowGlobal),
            (KeyCode::Char('4'), ApprovalChoice::DenyOnce),
            (KeyCode::Char('n'), ApprovalChoice::DenyOnce),
            (KeyCode::Char('5'), ApprovalChoice::StopTurn),
            (KeyCode::Char('c'), ApprovalChoice::StopTurn),
            (KeyCode::Esc, ApprovalChoice::StopTurn),
        ];
        for (code, expected) in cases {
            assert_eq!(ApprovalChoice::from_key(code), Some(expected));
        }
    }

    #[test]
    fn approval_choice_ignores_unrelated_keys() {
        assert!(ApprovalChoice::from_key(KeyCode::Char('z')).is_none());
        assert!(ApprovalChoice::from_key(KeyCode::Tab).is_none());
        assert!(ApprovalChoice::from_key(KeyCode::Enter).is_none());
    }

    #[test]
    fn approval_decisions_match_daemon_vocabulary() {
        assert_eq!(ApprovalChoice::AllowOnce.decision(), "allow");
        assert_eq!(ApprovalChoice::AllowSession.decision(), "allow_session");
        assert_eq!(ApprovalChoice::AllowGlobal.decision(), "allow_global");
        assert_eq!(ApprovalChoice::DenyOnce.decision(), "deny");
        assert_eq!(ApprovalChoice::StopTurn.decision(), "deny");
    }

    #[tokio::test]
    async fn approval_deny_clears_pending() {
        let mut app = App::new("0.1.0");
        app.pending_approval = Some(ApprovalRequest {
            id: "a1".into(),
            description: "rm".into(),
            pattern: "rm -rf".into(),
        });
        handle_key(&mut app, None, key(KeyCode::Char('n'), KeyModifiers::NONE))
            .await
            .unwrap();
        assert!(app.pending_approval.is_none());
    }

    #[tokio::test]
    async fn approval_stop_turn_clears_pending() {
        let mut app = App::new("0.1.0");
        app.pending_approval = Some(ApprovalRequest {
            id: "a1".into(),
            description: "rm".into(),
            pattern: "rm -rf".into(),
        });
        handle_key(&mut app, None, key(KeyCode::Esc, KeyModifiers::NONE))
            .await
            .unwrap();
        assert!(app.pending_approval.is_none());
    }
}
