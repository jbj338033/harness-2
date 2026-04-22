// IMPLEMENTS: D-260
//! Runbook-alert → skill mapping table. The first matching entry
//! wins, so a more specific service+severity row should come before
//! a catch-all on the same alert id.

use crate::alert::{AlertSeverity, IncidentAlert};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunbookSkillMap {
    pub entries: Vec<RunbookEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunbookEntry {
    pub alert_id_glob: String,
    pub min_severity: AlertSeverity,
    pub service_glob: String,
    pub skill_id: String,
}

#[must_use]
pub fn lookup_skill<'a>(map: &'a RunbookSkillMap, alert: &IncidentAlert) -> Option<&'a str> {
    map.entries
        .iter()
        .find(|e| {
            alert.severity >= e.min_severity
                && glob_matches(&e.alert_id_glob, &alert.id)
                && glob_matches(&e.service_glob, &alert.service)
        })
        .map(|e| e.skill_id.as_str())
}

fn glob_matches(glob: &str, value: &str) -> bool {
    if glob == "*" {
        return true;
    }
    if let Some(prefix) = glob.strip_suffix('*') {
        return value.starts_with(prefix);
    }
    glob == value
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn alert() -> IncidentAlert {
        IncidentAlert {
            id: "billing.5xx.spike".into(),
            severity: AlertSeverity::Critical,
            service: "billing-api".into(),
            labels: BTreeMap::new(),
            runbook_ref: None,
            fired_at_ms: 1,
        }
    }

    fn map() -> RunbookSkillMap {
        RunbookSkillMap {
            entries: vec![
                RunbookEntry {
                    alert_id_glob: "billing.5xx.*".into(),
                    min_severity: AlertSeverity::Error,
                    service_glob: "billing-*".into(),
                    skill_id: "skill.billing-runbook".into(),
                },
                RunbookEntry {
                    alert_id_glob: "*".into(),
                    min_severity: AlertSeverity::Page,
                    service_glob: "*".into(),
                    skill_id: "skill.generic-page".into(),
                },
            ],
        }
    }

    #[test]
    fn first_matching_wins() {
        let m = map();
        let s = lookup_skill(&m, &alert()).unwrap();
        assert_eq!(s, "skill.billing-runbook");
    }

    #[test]
    fn below_min_severity_skips_entry() {
        let m = map();
        let mut a = alert();
        a.severity = AlertSeverity::Info;
        assert!(lookup_skill(&m, &a).is_none());
    }

    #[test]
    fn fallback_glob_matches_unknown_service() {
        let m = map();
        let mut a = alert();
        a.id = "auth.outage".into();
        a.service = "auth-api".into();
        a.severity = AlertSeverity::Page;
        assert_eq!(lookup_skill(&m, &a).unwrap(), "skill.generic-page");
    }
}
