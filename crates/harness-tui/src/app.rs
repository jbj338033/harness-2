use harness_proto::Id;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

pub const CTRL_C_DOUBLE_PRESS_WINDOW: Duration = Duration::from_millis(800);

static COLOR_DISABLED: AtomicBool = AtomicBool::new(false);

pub fn disable_color() {
    COLOR_DISABLED.store(true, Ordering::Relaxed);
}

#[must_use]
pub fn color_enabled() -> bool {
    !COLOR_DISABLED.load(Ordering::Relaxed)
}

pub struct App {
    pub entries: Vec<Entry>,
    pub input: String,
    pub status: String,
    pub daemon_version: Option<String>,
    pub protocol_version: Option<u32>,
    pub connected: bool,
    pub session_id: Option<String>,
    pub agent_id: Option<String>,
    pub model: Option<String>,
    next_id: i64,
    pub overlay: Overlay,
    pub session_list: Vec<SessionRow>,
    pub config_list: Vec<(String, String)>,
    pub device_list: Vec<DeviceRow>,
    pub history: Vec<String>,
    pub history_cursor: Option<usize>,
    pub streams: HashMap<String, String>,
    pub pending_approval: Option<ApprovalRequest>,
    pub last_ctrl_c_at: Option<Instant>,
    pub skills: Vec<String>,
    pub continue_for_cwd: Option<String>,
    pub pending_writes: Vec<PendingWrite>,
    pub pending_chat: Option<String>,
    pub version: &'static str,
    pub turn_running: bool,
    pub committed_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PendingWrite {
    Resume(String),
    CreateForCwd(String),
    Chat(String),
}

#[derive(Clone)]
pub struct Entry {
    pub kind: EntryKind,
    pub text: String,
}

#[derive(Clone)]
pub enum EntryKind {
    Banner,
    User,
    Assistant,
    Daemon,
    System,
    Error,
    ToolCall,
}

#[derive(Clone)]
pub enum Overlay {
    None,
    Help,
    Sessions,
    Config,
    Devices,
    Agents,
}

#[derive(Clone)]
pub struct SessionRow {
    pub id: String,
    pub title: Option<String>,
    pub cwd: String,
}

#[derive(Clone)]
pub struct DeviceRow {
    pub id: String,
    pub name: String,
    pub last_seen_at: Option<i64>,
}

#[derive(Clone)]
pub struct ApprovalRequest {
    pub id: String,
    pub description: String,
    pub pattern: String,
}

impl App {
    #[must_use]
    pub fn new(version: &'static str) -> Self {
        let entries = vec![Entry {
            kind: EntryKind::Banner,
            text: version.to_string(),
        }];
        Self {
            entries,
            input: String::new(),
            status: "not connected".into(),
            daemon_version: None,
            protocol_version: None,
            connected: false,
            session_id: None,
            agent_id: None,
            model: None,
            next_id: 1,
            overlay: Overlay::None,
            session_list: Vec::new(),
            config_list: Vec::new(),
            device_list: Vec::new(),
            history: Vec::new(),
            history_cursor: None,
            streams: HashMap::new(),
            pending_approval: None,
            last_ctrl_c_at: None,
            skills: Vec::new(),
            continue_for_cwd: None,
            pending_writes: Vec::new(),
            pending_chat: None,
            version,
            turn_running: false,
            committed_count: 0,
        }
    }

    pub fn push_user(&mut self, text: impl Into<String>) {
        self.push(EntryKind::User, text);
    }
    pub fn push_assistant(&mut self, text: impl Into<String>) {
        self.push(EntryKind::Assistant, text);
    }
    pub fn push_daemon(&mut self, text: impl Into<String>) {
        self.push(EntryKind::Daemon, text);
    }
    pub fn push_system(&mut self, text: impl Into<String>) {
        self.push(EntryKind::System, text);
    }
    pub fn push_error(&mut self, text: impl Into<String>) {
        self.push(EntryKind::Error, text);
    }
    pub fn push_tool_call(&mut self, text: impl Into<String>) {
        self.push(EntryKind::ToolCall, text);
    }

    fn push(&mut self, kind: EntryKind, text: impl Into<String>) {
        self.entries.push(Entry {
            kind,
            text: text.into(),
        });
        self.trim();
    }

    fn trim(&mut self) {
        if self.entries.len() > 2000 {
            let drop = self.entries.len() - 2000;
            self.entries.drain(0..drop);
        }
    }

    pub fn take_input(&mut self) -> String {
        let taken = std::mem::take(&mut self.input);
        if !taken.is_empty() {
            self.history.push(taken.clone());
            if self.history.len() > 100 {
                self.history.remove(0);
            }
        }
        self.history_cursor = None;
        taken
    }

    pub fn next_id(&mut self) -> Id {
        let id = Id::Number(self.next_id);
        self.next_id += 1;
        id
    }

    pub fn mark_connected(&mut self, path: &str) {
        self.connected = true;
        self.status = format!("connected: {path}");
    }

    pub fn mark_disconnected(&mut self, backoff: Duration) {
        self.connected = false;
        self.status = format!(
            "disconnected — reconnecting in {}s",
            backoff.as_secs().max(1)
        );
        self.push_error("lost daemon connection");
    }

    pub fn mark_offline(&mut self, backoff: Duration) {
        self.status = format!("offline — next reconnect in {}s", backoff.as_secs().max(1));
    }

    pub fn history_prev(&mut self) {
        if self.history.is_empty() {
            return;
        }
        let next = match self.history_cursor {
            None => self.history.len().saturating_sub(1),
            Some(0) => 0,
            Some(n) => n - 1,
        };
        self.history_cursor = Some(next);
        self.input = self.history[next].clone();
    }

    pub fn history_next(&mut self) {
        match self.history_cursor {
            None => {}
            Some(n) if n + 1 >= self.history.len() => {
                self.history_cursor = None;
                self.input.clear();
            }
            Some(n) => {
                self.history_cursor = Some(n + 1);
                self.input = self.history[n + 1].clone();
            }
        }
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.entries.push(Entry {
            kind: EntryKind::Banner,
            text: self.version.to_string(),
        });
        self.committed_count = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_app_has_greeting() {
        let a = App::new("0.1.0");
        assert_eq!(a.entries.len(), 1);
    }

    #[test]
    fn take_input_moves_to_history() {
        let mut a = App::new("0.1.0");
        a.input = "hello".into();
        let s = a.take_input();
        assert_eq!(s, "hello");
        assert_eq!(a.history, vec!["hello"]);
        assert!(a.input.is_empty());
    }

    #[test]
    fn history_prev_then_next() {
        let mut a = App::new("0.1.0");
        a.input = "a".into();
        a.take_input();
        a.input = "b".into();
        a.take_input();
        assert!(a.input.is_empty());
        a.history_prev();
        assert_eq!(a.input, "b");
        a.history_prev();
        assert_eq!(a.input, "a");
        a.history_next();
        assert_eq!(a.input, "b");
        a.history_next();
        assert!(a.input.is_empty());
    }

    #[test]
    fn clear_keeps_one_system_entry() {
        let mut a = App::new("0.1.0");
        a.push_user("hi");
        a.push_user("there");
        a.clear();
        assert_eq!(a.entries.len(), 1);
    }

    #[test]
    fn no_color_env_disables() {
        super::disable_color();
        assert!(!super::color_enabled());
    }
}
