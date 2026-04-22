// IMPLEMENTS: D-251
//! Normalised `IncidentAlert` schema. Every alerting source — Datadog,
//! Grafana, custom webhook — is mapped onto this structure before it
//! enters the daemon.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlertSeverity {
    Info,
    Warning,
    Error,
    Critical,
    Page,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IncidentAlert {
    pub id: String,
    pub severity: AlertSeverity,
    pub service: String,
    pub labels: BTreeMap<String, String>,
    pub runbook_ref: Option<String>,
    pub fired_at_ms: i64,
}

impl IncidentAlert {
    #[must_use]
    pub fn must_page(&self) -> bool {
        matches!(self.severity, AlertSeverity::Critical | AlertSeverity::Page)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alert(sev: AlertSeverity) -> IncidentAlert {
        IncidentAlert {
            id: "a1".into(),
            severity: sev,
            service: "billing".into(),
            labels: BTreeMap::new(),
            runbook_ref: Some("runbook/billing-down".into()),
            fired_at_ms: 1,
        }
    }

    #[test]
    fn page_severity_pages() {
        assert!(alert(AlertSeverity::Page).must_page());
        assert!(alert(AlertSeverity::Critical).must_page());
    }

    #[test]
    fn warning_does_not_page() {
        assert!(!alert(AlertSeverity::Warning).must_page());
    }

    #[test]
    fn severity_ordering_info_lowest_page_highest() {
        assert!(AlertSeverity::Info < AlertSeverity::Warning);
        assert!(AlertSeverity::Critical < AlertSeverity::Page);
    }
}
