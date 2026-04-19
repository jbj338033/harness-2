use crate::app::{App, Overlay, PendingWrite};
use anyhow::Result;
use harness_proto::Request;
use serde_json::{Value, json};
use tokio::io::AsyncWriteExt;
use tokio::net::unix::OwnedWriteHalf;

#[derive(Debug, Clone, Copy)]
pub struct BuiltinCommand {
    pub name: &'static str,
    pub description: &'static str,
    pub usage: &'static str,
}

pub const COMMANDS: &[BuiltinCommand] = &[
    BuiltinCommand {
        name: "agents",
        description: "Show the agent tree overlay",
        usage: "",
    },
    BuiltinCommand {
        name: "cancel",
        description: "Cancel the current turn",
        usage: "",
    },
    BuiltinCommand {
        name: "clear",
        description: "Start a new session (clears the conversation)",
        usage: "",
    },
    BuiltinCommand {
        name: "config",
        description: "Manage config entries",
        usage: "[get|set|unset …]",
    },
    BuiltinCommand {
        name: "creds",
        description: "Manage provider credentials",
        usage: "[add PROV VAL | list | delete ID]",
    },
    BuiltinCommand {
        name: "devices",
        description: "List paired devices",
        usage: "",
    },
    BuiltinCommand {
        name: "help",
        description: "Show this help",
        usage: "",
    },
    BuiltinCommand {
        name: "list",
        description: "List sessions",
        usage: "",
    },
    BuiltinCommand {
        name: "model",
        description: "Get or set the active model",
        usage: "[id]",
    },
    BuiltinCommand {
        name: "pair",
        description: "Generate a pairing code",
        usage: "",
    },
    BuiltinCommand {
        name: "ping",
        description: "Round-trip probe",
        usage: "",
    },
    BuiltinCommand {
        name: "quit",
        description: "Exit the TUI",
        usage: "",
    },
    BuiltinCommand {
        name: "resume",
        description: "Resume a session (picker when no id)",
        usage: "[id]",
    },
    BuiltinCommand {
        name: "revoke",
        description: "Revoke a paired device",
        usage: "<id>",
    },
    BuiltinCommand {
        name: "status",
        description: "Daemon status probe",
        usage: "",
    },
    BuiltinCommand {
        name: "title",
        description: "Set the current session's title",
        usage: "<text>",
    },
];

pub enum Action {
    Quit,
    Done,
}

pub async fn dispatch(
    app: &mut App,
    writer: Option<&mut OwnedWriteHalf>,
    line: &str,
) -> Result<Action> {
    let trimmed = line.trim_start_matches('/');
    let mut parts = trimmed.split_whitespace();
    let cmd = parts.next().unwrap_or("");
    let rest: Vec<&str> = parts.collect();

    match cmd {
        "" => Ok(Action::Done),
        "quit" | "exit" | "q" => Ok(Action::Quit),
        "clear" => {
            cmd_clear(app);
            Ok(Action::Done)
        }
        "help" | "?" => {
            app.push_system(help_text());
            Ok(Action::Done)
        }
        "ping" => send(app, writer, "ping", None).await,
        "status" => send(app, writer, "status", None).await,
        "list" | "sessions" => cmd_sessions(app, writer).await,
        "resume" => cmd_resume(app, writer, &rest).await,
        "delete" => cmd_delete(app, writer, &rest).await,
        "title" => cmd_title(app, writer, &rest).await,
        "config" => cmd_config(app, writer, &rest).await,
        "model" => cmd_model(app, writer, &rest).await,
        "pair" => send(app, writer, "v1.auth.pair.new", None).await,
        "devices" => {
            app.overlay = Overlay::Devices;
            send(app, writer, "v1.device.list", None).await
        }
        "revoke" => cmd_revoke(app, writer, &rest).await,
        "agents" => {
            app.overlay = Overlay::Agents;
            Ok(Action::Done)
        }
        "cancel" => cmd_cancel(app, writer).await,
        "creds" => cmd_creds(app, writer, &rest).await,
        other => cmd_fallback(app, writer, other).await,
    }
}

fn cmd_clear(app: &mut App) {
    let cwd =
        std::env::current_dir().map_or_else(|_| ".".into(), |p| p.to_string_lossy().into_owned());
    app.clear();
    app.session_id = None;
    app.agent_id = None;
    app.pending_writes.push(PendingWrite::CreateForCwd(cwd));
}

async fn cmd_sessions(app: &mut App, writer: Option<&mut OwnedWriteHalf>) -> Result<Action> {
    app.overlay = Overlay::Sessions;
    send(app, writer, "v1.session.list", None).await
}

async fn cmd_resume(
    app: &mut App,
    writer: Option<&mut OwnedWriteHalf>,
    rest: &[&str],
) -> Result<Action> {
    if let Some(id) = rest.first() {
        send(
            app,
            writer,
            "v1.session.resume",
            Some(json!({ "session_id": id })),
        )
        .await
    } else {
        app.overlay = Overlay::Sessions;
        send(app, writer, "v1.session.list", None).await?;
        app.push_system("pick a session with /resume <id>");
        Ok(Action::Done)
    }
}

async fn cmd_delete(
    app: &mut App,
    writer: Option<&mut OwnedWriteHalf>,
    rest: &[&str],
) -> Result<Action> {
    let Some(id) = rest.first() else {
        app.push_error("usage: /delete <id>");
        return Ok(Action::Done);
    };
    send(
        app,
        writer,
        "v1.session.delete",
        Some(json!({ "session_id": id })),
    )
    .await
}

async fn cmd_title(
    app: &mut App,
    writer: Option<&mut OwnedWriteHalf>,
    rest: &[&str],
) -> Result<Action> {
    let Some(sid) = app.session_id.clone() else {
        app.push_error("no active session");
        return Ok(Action::Done);
    };
    if rest.is_empty() {
        app.push_error("usage: /title <text>");
        return Ok(Action::Done);
    }
    send(
        app,
        writer,
        "v1.config.set",
        Some(json!({
            "key": format!("session.{sid}.title"),
            "value": rest.join(" "),
        })),
    )
    .await
}

async fn cmd_config(
    app: &mut App,
    writer: Option<&mut OwnedWriteHalf>,
    rest: &[&str],
) -> Result<Action> {
    if rest.is_empty() {
        app.overlay = Overlay::Config;
        return send(app, writer, "v1.config.list", None).await;
    }
    match (rest.first().copied(), rest.len()) {
        (Some("set"), n) if n >= 3 => {
            let key = rest[1].to_string();
            let value = rest[2..].join(" ");
            send(
                app,
                writer,
                "v1.config.set",
                Some(json!({ "key": key, "value": value })),
            )
            .await
        }
        (Some("get"), 2) => {
            send(
                app,
                writer,
                "v1.config.get",
                Some(json!({ "key": rest[1] })),
            )
            .await
        }
        (Some("unset"), 2) => {
            send(
                app,
                writer,
                "v1.config.unset",
                Some(json!({ "key": rest[1] })),
            )
            .await
        }
        _ => {
            app.push_error(
                "usage: /config | /config get KEY | /config set KEY VALUE | /config unset KEY",
            );
            Ok(Action::Done)
        }
    }
}

async fn cmd_model(
    app: &mut App,
    writer: Option<&mut OwnedWriteHalf>,
    rest: &[&str],
) -> Result<Action> {
    if rest.is_empty() {
        app.push_system(format!(
            "current model: {}",
            app.model.as_deref().unwrap_or("unset")
        ));
        return Ok(Action::Done);
    }
    let value = rest[0].to_string();
    app.model = Some(value.clone());
    send(
        app,
        writer,
        "v1.config.set",
        Some(json!({ "key": "default_model", "value": value })),
    )
    .await
}

async fn cmd_revoke(
    app: &mut App,
    writer: Option<&mut OwnedWriteHalf>,
    rest: &[&str],
) -> Result<Action> {
    let Some(id) = rest.first() else {
        app.push_error("usage: /revoke <id>");
        return Ok(Action::Done);
    };
    send(app, writer, "v1.device.revoke", Some(json!({ "id": id }))).await
}

async fn cmd_cancel(app: &mut App, writer: Option<&mut OwnedWriteHalf>) -> Result<Action> {
    let Some(aid) = app.agent_id.clone() else {
        app.push_error("no active agent");
        return Ok(Action::Done);
    };
    send(
        app,
        writer,
        "v1.chat.cancel",
        Some(json!({ "agent_id": aid })),
    )
    .await
}

async fn cmd_creds(
    app: &mut App,
    writer: Option<&mut OwnedWriteHalf>,
    rest: &[&str],
) -> Result<Action> {
    match (rest.first().copied(), rest.len()) {
        (Some("add"), n) if n >= 3 => {
            send(
                app,
                writer,
                "v1.auth.credentials.add",
                Some(json!({ "provider": rest[1], "value": rest[2..].join(" ") })),
            )
            .await
        }
        (Some("list"), _) => send(app, writer, "v1.auth.credentials.list", None).await,
        (Some("delete"), 2) => {
            send(
                app,
                writer,
                "v1.auth.credentials.delete",
                Some(json!({ "id": rest[1] })),
            )
            .await
        }
        _ => {
            app.push_error("usage: /creds add PROVIDER VALUE | /creds list | /creds delete ID");
            Ok(Action::Done)
        }
    }
}

async fn cmd_fallback(
    app: &mut App,
    writer: Option<&mut OwnedWriteHalf>,
    other: &str,
) -> Result<Action> {
    if app.skills.iter().any(|s| s == other) {
        return activate_skill(app, writer, other.to_string()).await;
    }
    app.push_error(format!("unknown command: /{other} — try /help"));
    Ok(Action::Done)
}

async fn activate_skill(
    app: &mut App,
    writer: Option<&mut OwnedWriteHalf>,
    name: String,
) -> Result<Action> {
    send(
        app,
        writer,
        "v1.skill.activate",
        Some(json!({ "name": name })),
    )
    .await
}

async fn send(
    app: &mut App,
    writer: Option<&mut OwnedWriteHalf>,
    method: &str,
    params: Option<Value>,
) -> Result<Action> {
    let Some(w) = writer else {
        app.push_error("not connected");
        return Ok(Action::Done);
    };
    let id = app.next_id();
    let req = Request::new(id, method, params);
    let text = serde_json::to_string(&req)?;
    w.write_all(text.as_bytes()).await?;
    w.write_all(b"\n").await?;
    Ok(Action::Done)
}

#[must_use]
pub fn help_text() -> String {
    use std::fmt::Write;
    let mut s = String::from("commands:\n");
    for c in COMMANDS {
        let name_and_usage = if c.usage.is_empty() {
            format!("/{}", c.name)
        } else {
            format!("/{} {}", c.name, c.usage)
        };
        writeln!(s, "  {name_and_usage:22}  {}", c.description).unwrap();
    }
    s.push_str(
        "\nskill commands (type /<skill-name>):\n  activate a discovered skill (see /help).",
    );
    s.trim_end().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn unknown_slash_is_reported() {
        let mut app = App::new("0.1.0");
        dispatch(&mut app, None, "/bogus").await.unwrap();
        assert!(
            app.entries
                .iter()
                .any(|e| e.text.contains("unknown command"))
        );
    }

    #[tokio::test]
    async fn help_prints_reference() {
        let mut app = App::new("0.1.0");
        dispatch(&mut app, None, "/help").await.unwrap();
        assert!(app.entries.iter().any(|e| e.text.contains("/clear")));
        assert!(app.entries.iter().any(|e| e.text.contains("/resume")));
    }

    #[tokio::test]
    async fn quit_returns_quit() {
        let mut app = App::new("0.1.0");
        let act = dispatch(&mut app, None, "/quit").await.unwrap();
        assert!(matches!(act, Action::Quit));
    }

    #[tokio::test]
    async fn not_connected_errors_on_send() {
        let mut app = App::new("0.1.0");
        dispatch(&mut app, None, "/ping").await.unwrap();
        assert!(
            app.entries
                .iter()
                .any(|e| matches!(e.kind, crate::app::EntryKind::Error))
        );
    }

    #[tokio::test]
    async fn clear_queues_new_session() {
        let mut app = App::new("0.1.0");
        app.session_id = Some("existing".into());
        dispatch(&mut app, None, "/clear").await.unwrap();
        assert!(app.session_id.is_none());
        assert!(matches!(
            app.pending_writes.first(),
            Some(PendingWrite::CreateForCwd(_))
        ));
    }

    #[tokio::test]
    async fn commands_table_sorted_alphabetically() {
        for pair in COMMANDS.windows(2) {
            assert!(
                pair[0].name < pair[1].name,
                "COMMANDS not alphabetized at `{}` vs `{}`",
                pair[0].name,
                pair[1].name
            );
        }
    }
}
