// IMPLEMENTS: D-244
//! Task projection row. Materialised view of the events stream so
//! the wave coordinator + Web/TUI can query "what's pending /
//! running / blocked" cheaply.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Pending,
    Running,
    Blocked,
    Completed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskRow {
    pub id: String,
    pub correlation_id: String,
    pub title: String,
    pub status: TaskStatus,
    pub depends_on: Vec<String>,
    pub files_modified: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_round_trips_via_serde() {
        let row = TaskRow {
            id: "t1".into(),
            correlation_id: "c1".into(),
            title: "implement X".into(),
            status: TaskStatus::Running,
            depends_on: vec![],
            files_modified: vec!["src/x.rs".into()],
        };
        let s = serde_json::to_string(&row).unwrap();
        assert!(s.contains("\"running\""));
        let back: TaskRow = serde_json::from_str(&s).unwrap();
        assert_eq!(back, row);
    }
}
