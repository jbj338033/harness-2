// IMPLEMENTS: D-300
//! Shape of the `harness audit bias` invocation. The CLI surface
//! parses argv into this; this module owns the schema so other
//! callers (Web, scheduled jobs) can build the same struct directly.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BiasAuditFormat {
    Json,
    NycLl144,
    EuFria,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BiasAuditInvocation {
    pub session_id: Option<String>,
    pub since_iso: Option<String>,
    pub format: BiasAuditFormat,
    pub include_cohorts: Vec<String>,
}

#[must_use]
pub fn parse_invocation(args: &[&str]) -> BiasAuditInvocation {
    let mut inv = BiasAuditInvocation {
        session_id: None,
        since_iso: None,
        format: BiasAuditFormat::Json,
        include_cohorts: Vec::new(),
    };
    let mut i = 0;
    while i < args.len() {
        match args[i] {
            "--session" => {
                if let Some(v) = args.get(i + 1) {
                    inv.session_id = Some((*v).to_string());
                    i += 2;
                    continue;
                }
            }
            "--since" => {
                if let Some(v) = args.get(i + 1) {
                    inv.since_iso = Some((*v).to_string());
                    i += 2;
                    continue;
                }
            }
            "--format" => {
                if let Some(v) = args.get(i + 1) {
                    inv.format = match *v {
                        "nyc_ll144" => BiasAuditFormat::NycLl144,
                        "eu_fria" => BiasAuditFormat::EuFria,
                        _ => BiasAuditFormat::Json,
                    };
                    i += 2;
                    continue;
                }
            }
            "--cohort" => {
                if let Some(v) = args.get(i + 1) {
                    inv.include_cohorts.push((*v).to_string());
                    i += 2;
                    continue;
                }
            }
            _ => {}
        }
        i += 1;
    }
    inv
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_json() {
        let inv = parse_invocation(&[]);
        assert_eq!(inv.format, BiasAuditFormat::Json);
        assert!(inv.session_id.is_none());
    }

    #[test]
    fn parses_nyc_format_and_cohorts() {
        let inv = parse_invocation(&[
            "--format",
            "nyc_ll144",
            "--cohort",
            "race",
            "--cohort",
            "age",
        ]);
        assert_eq!(inv.format, BiasAuditFormat::NycLl144);
        assert_eq!(inv.include_cohorts, vec!["race", "age"]);
    }

    #[test]
    fn parses_session_and_since() {
        let inv = parse_invocation(&["--session", "s1", "--since", "2026-01-01"]);
        assert_eq!(inv.session_id.as_deref(), Some("s1"));
        assert_eq!(inv.since_iso.as_deref(), Some("2026-01-01"));
    }
}
