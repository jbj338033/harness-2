use crate::app::{App, ApprovalRequest, DeviceRow, SessionRow};
use harness_proto::{Response, ResponsePayload};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::net::unix::OwnedReadHalf;
use tokio::sync::mpsc;
use tracing::debug;

pub async fn reader_task(r: OwnedReadHalf, tx: mpsc::Sender<String>) {
    let mut lines = BufReader::new(r).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        let line = line.trim().to_string();
        if line.is_empty() {
            continue;
        }
        if tx.send(line).await.is_err() {
            break;
        }
    }
}

pub fn handle_daemon_line(app: &mut App, line: &str) {
    let Ok(v) = serde_json::from_str::<Value>(line) else {
        app.push_daemon(format!("← raw: {line}"));
        return;
    };

    if v.get("id").is_none() {
        handle_notification(app, &v);
        return;
    }

    match serde_json::from_value::<Response>(v.clone()) {
        Ok(resp) => handle_response(app, resp),
        Err(_) => {
            app.push_daemon(format!("← raw: {line}"));
        }
    }
}

fn handle_response(app: &mut App, resp: Response) {
    match resp.payload {
        ResponsePayload::Result(v) => handle_result(app, &v),
        ResponsePayload::Error(e) => {
            app.turn_running = false;
            app.push_error(format!("← error {}: {}", e.code, e.message));
        }
    }
}

fn short_id6(id: &str) -> &str {
    let n = id.len().saturating_sub(6);
    &id[n..]
}

const MAX_TOOL_LINE_COLS: usize = 80;

fn one_line(s: &str) -> String {
    use ratatui::text::Span;
    let collapsed: String = s.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut out = String::new();
    let mut cols = 0usize;
    for ch in collapsed.chars() {
        let w = Span::raw(ch.to_string()).width();
        if cols + w > MAX_TOOL_LINE_COLS {
            out.push('…');
            return out;
        }
        out.push(ch);
        cols += w;
    }
    out
}

fn format_tool_args(name: &str, input_preview: &str) -> String {
    let Ok(v) = serde_json::from_str::<Value>(input_preview) else {
        return one_line(input_preview);
    };
    let Some(obj) = v.as_object() else {
        return one_line(input_preview);
    };

    let primary_keys: &[&str] = match name {
        "bash" | "shell" => &["command"],
        "read" | "write" | "edit" | "create" | "delete" | "ls" | "cat" => &["path"],
        "glob" | "grep" | "search" | "rg" => &["pattern", "path"],
        "web_fetch" | "fetch" => &["url"],
        "lsp" => &["action", "symbol", "path"],
        "browser" => &["action", "url"],
        "computer_use" | "computer" => &["action"],
        "spawn" => &["name", "goal"],
        "cancel" | "wait" => &["agent_id"],
        "activate_skill" | "skill" => &["name"],
        _ => &[],
    };

    let mut parts: Vec<String> = primary_keys
        .iter()
        .filter_map(|k| obj.get(*k).and_then(stringify_scalar))
        .collect();

    if parts.is_empty() {
        for (k, v) in obj {
            if let Some(s) = stringify_scalar(v) {
                parts.push(format!("{k}={s}"));
                if parts.len() >= 2 {
                    break;
                }
            }
        }
    }

    if parts.is_empty() {
        one_line(input_preview)
    } else {
        one_line(&parts.join("  "))
    }
}

fn stringify_scalar(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

fn handle_result(app: &mut App, v: &Value) {
    apply_scalar_fields(app, v);
    apply_sessions(app, v);
    apply_skills(app, v);
    apply_credentials(app, v);
    apply_entries(app, v);
    apply_devices(app, v);
    apply_pairing(app, v);
    apply_status(app, v);

    if !is_known_shape(v) {
        app.push_daemon(format!(
            "← {}",
            serde_json::to_string(v).unwrap_or_default()
        ));
    }
}

fn apply_scalar_fields(app: &mut App, v: &Value) {
    if let Some(ver) = v.get("version").and_then(Value::as_str) {
        app.daemon_version = Some(ver.into());
    }
    if let Some(sel) = v.get("selected").and_then(Value::as_u64) {
        app.protocol_version = Some(u32::try_from(sel).unwrap_or(u32::MAX));
    }
    if let Some(sid) = v.get("session_id").and_then(Value::as_str) {
        let was_unset = app.session_id.is_none();
        app.session_id = Some(sid.into());
        if was_unset && let Some(msg) = app.pending_chat.take() {
            app.pending_writes.push(crate::app::PendingWrite::Chat(msg));
        }
    }
    if let Some(aid) = v.get("agent_id").and_then(Value::as_str) {
        app.agent_id = Some(aid.into());
    }
    if let Some(model) = v.get("model").and_then(Value::as_str) {
        app.model = Some(model.into());
    }
}

fn apply_sessions(app: &mut App, v: &Value) {
    let Some(arr) = v.get("sessions").and_then(Value::as_array) else {
        return;
    };
    app.session_list = arr
        .iter()
        .filter_map(|row| {
            Some(SessionRow {
                id: row.get("id").and_then(Value::as_str)?.to_string(),
                title: row.get("title").and_then(Value::as_str).map(str::to_string),
                cwd: row.get("cwd").and_then(Value::as_str)?.to_string(),
            })
        })
        .collect();

    if let Some(cwd) = app.continue_for_cwd.take() {
        if let Some(row) = app.session_list.iter().find(|r| r.cwd == cwd).cloned() {
            let short = short_id6(&row.id);
            app.push_system(format!("resuming most-recent session for {cwd}: …{short}"));
            app.pending_writes
                .push(crate::app::PendingWrite::Resume(row.id));
        } else {
            app.push_system("no prior session for this directory — creating a new one");
            app.pending_writes
                .push(crate::app::PendingWrite::CreateForCwd(cwd));
        }
    }
}

fn apply_skills(app: &mut App, v: &Value) {
    if let Some(arr) = v.get("skills").and_then(Value::as_array) {
        app.skills = arr
            .iter()
            .filter_map(|row| row.get("name").and_then(Value::as_str).map(str::to_string))
            .collect();
    }
}

fn apply_credentials(app: &mut App, v: &Value) {
    if let Some(arr) = v.get("credentials").and_then(Value::as_array)
        && arr.is_empty()
    {
        app.push_system("no credentials yet — exit and run `harness auth login` to add one");
    }
}

fn apply_entries(app: &mut App, v: &Value) {
    if let Some(arr) = v.get("entries").and_then(Value::as_array) {
        app.config_list = arr
            .iter()
            .filter_map(|row| {
                Some((
                    row.get("key").and_then(Value::as_str)?.to_string(),
                    row.get("value").and_then(Value::as_str)?.to_string(),
                ))
            })
            .collect();
    }
}

fn apply_devices(app: &mut App, v: &Value) {
    if let Some(arr) = v.get("devices").and_then(Value::as_array) {
        app.device_list = arr
            .iter()
            .filter_map(|row| {
                Some(DeviceRow {
                    id: row.get("id").and_then(Value::as_str)?.to_string(),
                    name: row.get("name").and_then(Value::as_str)?.to_string(),
                    last_seen_at: row.get("last_seen_at").and_then(Value::as_i64),
                })
            })
            .collect();
    }
}

fn apply_pairing(app: &mut App, v: &Value) {
    if let Some(code) = v.get("code").and_then(Value::as_str) {
        let fp = v
            .get("fingerprint")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let port = v.get("port").and_then(Value::as_u64).unwrap_or(8384);
        app.push_system(format!(
            "pairing code: {code}\n  port: {port}\n  fingerprint: {fp}"
        ));
    }
}

fn apply_status(app: &mut App, v: &Value) {
    let Some(status) = v.get("status").and_then(Value::as_str) else {
        return;
    };
    app.turn_running = false;
    if status == "failed"
        && let Some(reason) = v.get("reason").and_then(Value::as_str)
    {
        app.push_error(format!("turn failed: {reason}"));
    }
}

fn is_known_shape(v: &Value) -> bool {
    v.get("version").is_some()
        || v.get("selected").is_some()
        || v.get("session_id").is_some()
        || v.get("sessions").is_some()
        || v.get("skills").is_some()
        || v.get("credentials").is_some()
        || v.get("entries").is_some()
        || v.get("devices").is_some()
        || v.get("code").is_some()
        || v.get("status").is_some()
        || v.get("deleted").is_some()
        || v.get("removed").is_some()
        || v.get("key").is_some()
        || v.get("id").is_some()
}

fn handle_notification(app: &mut App, v: &Value) {
    let method = v.get("method").and_then(Value::as_str).unwrap_or("");
    let params = v.get("params").cloned().unwrap_or(Value::Null);

    match method {
        "stream.delta" => stream_delta(app, &params),
        "stream.done" => stream_done(app, &params),
        "stream.error" => stream_error(app, &params),
        "stream.tool_call" => stream_tool_call(app, &params),
        "stream.tool_result" => stream_tool_result(app, &params),
        "agent.status" => agent_status(app, &params),
        "approval.request" => approval_request(app, &params),
        other => debug!(method = other, "unhandled notification"),
    }
}

fn stream_delta(app: &mut App, params: &Value) {
    let Some(mid) = params.get("message_id").and_then(Value::as_str) else {
        return;
    };
    let Some(content) = params.get("content").and_then(Value::as_str) else {
        return;
    };
    let buf = {
        let b = app.streams.entry(mid.into()).or_default();
        b.push_str(content);
        b.clone()
    };
    if matches!(
        app.entries.last().map(|e| &e.kind),
        Some(crate::app::EntryKind::Assistant)
    ) {
        if let Some(last) = app.entries.last_mut() {
            last.text = buf;
        }
    } else {
        app.push_assistant(buf);
    }
}

fn stream_done(app: &mut App, params: &Value) {
    let mid = params
        .get("message_id")
        .and_then(Value::as_str)
        .unwrap_or_default();
    app.streams.remove(mid);
}

fn stream_error(app: &mut App, params: &Value) {
    app.turn_running = false;
    let reason = params
        .get("reason")
        .and_then(Value::as_str)
        .unwrap_or("stream error");
    app.push_error(format!("stream error: {reason}"));
}

fn stream_tool_call(app: &mut App, params: &Value) {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let args = params
        .get("input_preview")
        .and_then(Value::as_str)
        .map(|p| format_tool_args(name, p))
        .filter(|s| !s.is_empty());
    let line = args.map_or_else(|| name.to_string(), |a| format!("{name}  {a}"));
    app.push_tool_call(line);
}

fn stream_tool_result(app: &mut App, params: &Value) {
    let is_error = params
        .get("is_error")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if !is_error {
        return;
    }
    let out = params
        .get("output")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let msg = one_line(out);
    if msg.is_empty() {
        app.push_error("tool failed");
    } else {
        app.push_error(format!("tool failed: {msg}"));
    }
}

fn agent_status(app: &mut App, params: &Value) {
    let status = params
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or_default();
    match status {
        "running" | "started" | "streaming" => app.turn_running = true,
        "done" | "completed" | "cancelled" | "failed" => app.turn_running = false,
        _ => {}
    }
}

fn approval_request(app: &mut App, params: &Value) {
    if let (Some(id), Some(desc), Some(pat)) = (
        params.get("id").and_then(Value::as_str),
        params.get("description").and_then(Value::as_str),
        params.get("pattern").and_then(Value::as_str),
    ) {
        app.pending_approval = Some(ApprovalRequest {
            id: id.into(),
            description: desc.into(),
            pattern: pat.into(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;
    use serde_json::json;

    #[test]
    fn stream_delta_appends_to_assistant_entry() {
        let mut app = App::new("0.1.0");
        let n = json!({
            "jsonrpc": "2.0",
            "method": "stream.delta",
            "params": {"message_id": "m1", "content": "hello "}
        });
        handle_daemon_line(&mut app, &n.to_string());
        let n2 = json!({
            "jsonrpc": "2.0",
            "method": "stream.delta",
            "params": {"message_id": "m1", "content": "world"}
        });
        handle_daemon_line(&mut app, &n2.to_string());
        let last = app.entries.last().unwrap();
        assert!(matches!(last.kind, crate::app::EntryKind::Assistant));
        assert_eq!(last.text, "hello world");
    }

    #[test]
    fn stream_done_clears_buffer() {
        let mut app = App::new("0.1.0");
        handle_daemon_line(
            &mut app,
            &json!({
                "jsonrpc": "2.0",
                "method": "stream.delta",
                "params": {"message_id": "m1", "content": "x"}
            })
            .to_string(),
        );
        handle_daemon_line(
            &mut app,
            &json!({
                "jsonrpc": "2.0",
                "method": "stream.done",
                "params": {"message_id": "m1"}
            })
            .to_string(),
        );
        assert!(!app.streams.contains_key("m1"));
    }

    #[test]
    fn result_captures_version() {
        let mut app = App::new("0.1.0");
        handle_daemon_line(
            &mut app,
            &json!({
                "jsonrpc": "2.0",
                "id": 1,
                "result": {"pong": true, "version": "0.2.0"}
            })
            .to_string(),
        );
        assert_eq!(app.daemon_version.as_deref(), Some("0.2.0"));
    }

    #[test]
    fn error_response_produces_error_entry() {
        let mut app = App::new("0.1.0");
        let before = app.entries.len();
        handle_daemon_line(
            &mut app,
            &json!({
                "jsonrpc": "2.0",
                "id": 1,
                "error": {"code": -32001, "message": "nope"}
            })
            .to_string(),
        );
        assert!(app.entries.len() > before);
        assert!(matches!(
            app.entries.last().unwrap().kind,
            crate::app::EntryKind::Error
        ));
    }

    #[test]
    fn session_list_result_cached() {
        let mut app = App::new("0.1.0");
        handle_daemon_line(
            &mut app,
            &json!({
                "jsonrpc": "2.0",
                "id": 1,
                "result": {
                    "sessions": [
                        {"id": "s1", "title": null, "cwd": "/tmp", "updated_at": 123}
                    ]
                }
            })
            .to_string(),
        );
        assert_eq!(app.session_list.len(), 1);
        assert_eq!(app.session_list[0].cwd, "/tmp");
    }

    #[test]
    fn approval_request_stored() {
        let mut app = App::new("0.1.0");
        handle_daemon_line(
            &mut app,
            &json!({
                "jsonrpc": "2.0",
                "method": "approval.request",
                "params": {"id": "a1", "description": "rm", "pattern": "rm -rf"}
            })
            .to_string(),
        );
        assert!(app.pending_approval.is_some());
    }

    #[test]
    fn tool_call_shows_name_and_args_only() {
        let mut app = App::new("0.1.0");
        let before = app.entries.len();
        handle_daemon_line(
            &mut app,
            &json!({
                "jsonrpc": "2.0",
                "method": "stream.tool_call",
                "params": {
                    "session_id": "s",
                    "message_id": "m",
                    "tool_call_id": "t",
                    "name": "read",
                    "input_preview": "{\"path\": \"src/main.rs\"}"
                }
            })
            .to_string(),
        );
        assert_eq!(app.entries.len(), before + 1);
        let last = app.entries.last().unwrap();
        assert!(matches!(last.kind, crate::app::EntryKind::ToolCall));
        assert!(last.text.contains("read"));
        assert!(last.text.contains("src/main.rs"));
    }

    #[test]
    fn tool_result_success_is_silent() {
        let mut app = App::new("0.1.0");
        let before = app.entries.len();
        handle_daemon_line(
            &mut app,
            &json!({
                "jsonrpc": "2.0",
                "method": "stream.tool_result",
                "params": {
                    "session_id": "s",
                    "tool_call_id": "t",
                    "output": "file contents spanning\nmany lines\n...",
                    "is_error": false
                }
            })
            .to_string(),
        );
        assert_eq!(
            app.entries.len(),
            before,
            "successful tool results must not push timeline entries"
        );
    }

    #[test]
    fn tool_result_error_pushes_single_line() {
        let mut app = App::new("0.1.0");
        let before = app.entries.len();
        handle_daemon_line(
            &mut app,
            &json!({
                "jsonrpc": "2.0",
                "method": "stream.tool_result",
                "params": {
                    "session_id": "s",
                    "tool_call_id": "t",
                    "output": "permission denied\nstacktrace: …",
                    "is_error": true
                }
            })
            .to_string(),
        );
        assert_eq!(app.entries.len(), before + 1);
        let last = app.entries.last().unwrap();
        assert!(matches!(last.kind, crate::app::EntryKind::Error));
        assert!(!last.text.contains('\n'));
    }

    #[test]
    fn bash_tool_shows_only_command() {
        let out = format_tool_args("bash", r#"{"command":"ls","cwd":".","timeout_secs":30}"#);
        assert_eq!(out, "ls");
    }

    #[test]
    fn read_tool_shows_only_path() {
        let out = format_tool_args("read", r#"{"path":"src/main.rs"}"#);
        assert_eq!(out, "src/main.rs");
    }

    #[test]
    fn grep_tool_shows_pattern_then_path() {
        let out = format_tool_args(
            "grep",
            r#"{"pattern":"todo","path":"src","case_insensitive":true}"#,
        );
        assert_eq!(out, "todo src");
    }

    #[test]
    fn unknown_tool_falls_back_to_key_value() {
        let out = format_tool_args("mystery", r#"{"x":"foo","count":3}"#);
        assert!(out.contains("x=foo") || out.contains("count=3"));
    }

    #[test]
    fn non_json_preview_is_passed_through_cleanly() {
        let out = format_tool_args("bash", "ls -la (literal)");
        assert!(out.contains("ls -la"));
    }

    #[test]
    fn one_line_caps_wide_chars_to_display_budget() {
        let long = "가".repeat(200);
        let out = one_line(&long);
        assert!(out.ends_with('…'));
        assert!(out.chars().count() <= MAX_TOOL_LINE_COLS / 2 + 1);
    }
}
