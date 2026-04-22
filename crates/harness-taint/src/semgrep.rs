// IMPLEMENTS: D-351
//! Thin wrapper over the `semgrep` CLI. We don't bundle Semgrep — too
//! heavy and the rule pack updates frequently — so this module just
//! parses the JSON output of an external invocation. Callers who want
//! the SAST-Genius low-FP rule set ship it via `--config p/llm`.

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SemgrepError {
    #[error("semgrep json parse: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("semgrep returned no `results` field")]
    Malformed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SemgrepFinding {
    pub check_id: String,
    pub path: String,
    pub start_line: u32,
    pub end_line: u32,
    pub severity: String,
    pub message: String,
}

/// Parse the JSON output of `semgrep --json`. Returns the high-confidence
/// findings only — D-351 is explicit about avoiding the noisy `INFO`
/// tier so the operator only sees actionable hits.
pub struct SemgrepWrapper;

impl SemgrepWrapper {
    pub fn parse(json: &str) -> Result<Vec<SemgrepFinding>, SemgrepError> {
        let v: serde_json::Value = serde_json::from_str(json)?;
        let results = v
            .get("results")
            .and_then(|r| r.as_array())
            .ok_or(SemgrepError::Malformed)?;
        let mut out = Vec::with_capacity(results.len());
        for r in results {
            let check_id = r
                .get("check_id")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("?")
                .to_string();
            let path = r
                .get("path")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("")
                .to_string();
            let start_line = r
                .pointer("/start/line")
                .and_then(serde_json::Value::as_u64)
                .and_then(|n| u32::try_from(n).ok())
                .unwrap_or(0);
            let end_line = r
                .pointer("/end/line")
                .and_then(serde_json::Value::as_u64)
                .and_then(|n| u32::try_from(n).ok())
                .unwrap_or(0);
            let severity = r
                .pointer("/extra/severity")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("INFO")
                .to_string();
            let message = r
                .pointer("/extra/message")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("")
                .to_string();
            // D-351: drop INFO; keep WARNING / ERROR.
            if severity.eq_ignore_ascii_case("INFO") {
                continue;
            }
            out.push(SemgrepFinding {
                check_id,
                path,
                start_line,
                end_line,
                severity,
                message,
            });
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
        "results": [
            {
                "check_id": "rust.shell-exec-with-user-input",
                "path": "src/main.rs",
                "start": {"line": 10},
                "end": {"line": 12},
                "extra": {
                    "severity": "ERROR",
                    "message": "User input flows into Command::new"
                }
            },
            {
                "check_id": "rust.maybe-untested",
                "path": "src/lib.rs",
                "start": {"line": 5},
                "end": {"line": 5},
                "extra": {
                    "severity": "INFO",
                    "message": "Function lacks a doc comment"
                }
            }
        ]
    }"#;

    #[test]
    fn parses_high_confidence_findings_only() {
        let findings = SemgrepWrapper::parse(SAMPLE).unwrap();
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].check_id, "rust.shell-exec-with-user-input");
        assert_eq!(findings[0].severity, "ERROR");
    }

    #[test]
    fn malformed_json_errors_gracefully() {
        let err = SemgrepWrapper::parse("not json").unwrap_err();
        assert!(matches!(err, SemgrepError::Parse(_)));
    }

    #[test]
    fn missing_results_field_is_malformed() {
        let err = SemgrepWrapper::parse(r#"{"version":"x"}"#).unwrap_err();
        assert!(matches!(err, SemgrepError::Malformed));
    }
}
