// IMPLEMENTS: D-253
//! `Incident` — first-class entity with lifecycle, commander, and a
//! pointer to the eventual postmortem document.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IncidentStatus {
    Triggered,
    Acknowledged,
    Investigating,
    Mitigated,
    Resolved,
    Postmortem,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimelineEntry {
    pub at_ms: i64,
    pub status: IncidentStatus,
    pub note: String,
    pub actor: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Incident {
    pub id: String,
    pub title: String,
    pub status: IncidentStatus,
    pub commander: Option<String>,
    pub timeline: Vec<TimelineEntry>,
    pub postmortem_ref: Option<String>,
}

impl Incident {
    pub fn transition(&mut self, status: IncidentStatus, note: String, actor: String, at_ms: i64) {
        self.status = status;
        self.timeline.push(TimelineEntry {
            at_ms,
            status,
            note,
            actor,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lifecycle_walks_to_postmortem() {
        let mut i = Incident {
            id: "inc-1".into(),
            title: "billing 5xx spike".into(),
            status: IncidentStatus::Triggered,
            commander: None,
            timeline: vec![],
            postmortem_ref: None,
        };
        for s in [
            IncidentStatus::Acknowledged,
            IncidentStatus::Investigating,
            IncidentStatus::Mitigated,
            IncidentStatus::Resolved,
            IncidentStatus::Postmortem,
        ] {
            i.transition(s, "step".into(), "ic".into(), 1);
        }
        assert_eq!(i.status, IncidentStatus::Postmortem);
        assert_eq!(i.timeline.len(), 5);
    }
}
