// IMPLEMENTS: D-169
//! Strict-schema import + role mapping + VCS commit compatibility check.
//! D-170(d) carved out a hole for forward-compat: top-level fields are
//! strict (unknown rejected), but payload bodies preserve unknowns so a
//! newer producer doesn't break an older reader.

use crate::TraceError;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CompatError {
    #[error("unknown top-level field: {0}")]
    UnknownTopField(String),
    #[error("missing required field: {0}")]
    MissingField(&'static str),
    #[error("repo ref {trace} does not descend from current sha {current}")]
    RepoRefDiverged { trace: String, current: String },
    #[error("unknown external role: {0}")]
    UnknownRole(String),
}

/// Allowed top-level fields per the trace spec — anything outside this set
/// raises [`CompatError::UnknownTopField`] in strict mode.
const ALLOWED_TOP_FIELDS: &[&str] = &[
    "spec_version",
    "session_id",
    "payload",
    "public_key",
    "signature",
    "key_id",
    "repo_ref",
];

/// Strictly validate the top-level field set. Caller passes a parsed JSON
/// object; this returns Ok if every key is in `ALLOWED_TOP_FIELDS` and the
/// required ones are present.
pub fn validate_top_level(value: &serde_json::Value) -> Result<(), CompatError> {
    let map = value
        .as_object()
        .ok_or(CompatError::MissingField("top-level object"))?;
    for required in [
        "spec_version",
        "session_id",
        "payload",
        "public_key",
        "signature",
    ] {
        if !map.contains_key(required) {
            return Err(CompatError::MissingField(required));
        }
    }
    for key in map.keys() {
        if !ALLOWED_TOP_FIELDS.contains(&key.as_str()) {
            return Err(CompatError::UnknownTopField(key.clone()));
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HarnessActor {
    HumanUser,
    AgentAssistant,
    SystemPrompt,
    ToolResult,
}

/// Translate an external trace's role string (Cursor, Cognition, generic
/// OpenAI) into a Harness actor. Unknown roles raise an error so the
/// importer can quarantine rather than guess.
pub fn map_external_role(external: &str) -> Result<HarnessActor, CompatError> {
    let actor = match external.to_ascii_lowercase().as_str() {
        "user" | "human" | "end_user" => HarnessActor::HumanUser,
        "assistant" | "agent" | "model" | "ai" => HarnessActor::AgentAssistant,
        "system" | "developer" | "instructions" => HarnessActor::SystemPrompt,
        "tool" | "function" | "tool_result" => HarnessActor::ToolResult,
        other => return Err(CompatError::UnknownRole(other.into())),
    };
    Ok(actor)
}

/// VCS compatibility — the trace's `repo_ref` (commit sha or "branch@sha")
/// must either match the current sha exactly or be a prefix of it (so a
/// short trace ref accepts a full local sha).
pub fn verify_repo_ref(trace_ref: &str, current_sha: &str) -> Result<(), CompatError> {
    let trace = strip_branch_prefix(trace_ref);
    let current = current_sha.trim().to_ascii_lowercase();
    let trace_norm = trace.trim().to_ascii_lowercase();
    if trace_norm.is_empty() {
        return Err(CompatError::MissingField("repo_ref"));
    }
    if current.is_empty() {
        return Err(CompatError::MissingField("current_sha"));
    }
    if current.starts_with(&trace_norm) || trace_norm.starts_with(&current) {
        return Ok(());
    }
    Err(CompatError::RepoRefDiverged {
        trace: trace_ref.into(),
        current: current_sha.into(),
    })
}

fn strip_branch_prefix(s: &str) -> &str {
    s.split_once('@').map_or(s, |(_, sha)| sha)
}

/// Convenience: parse the bytes of a trace file in strict mode. Wraps the
/// JSON parse so a caller can adopt strict imports without re-implementing
/// the byte → object → validate pipeline.
pub fn parse_strict(bytes: &[u8]) -> Result<serde_json::Value, TraceError> {
    let v: serde_json::Value = serde_json::from_slice(bytes)?;
    validate_top_level(&v).map_err(|e| {
        TraceError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            e.to_string(),
        ))
    })?;
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn well_formed_object() -> serde_json::Value {
        let pk: Vec<u8> = vec![0u8; 32];
        let sig: Vec<u8> = vec![0u8; 64];
        json!({
            "spec_version": "trace/0.2",
            "session_id": "s",
            "payload": {},
            "public_key": pk,
            "signature": sig,
        })
    }

    #[test]
    fn strict_accepts_known_fields_only() {
        let v = well_formed_object();
        validate_top_level(&v).unwrap();
    }

    #[test]
    fn strict_rejects_unknown_top_field() {
        let mut v = well_formed_object();
        v.as_object_mut()
            .unwrap()
            .insert("attacker".into(), json!("payload"));
        let err = validate_top_level(&v).unwrap_err();
        assert!(matches!(err, CompatError::UnknownTopField(ref f) if f == "attacker"));
    }

    #[test]
    fn strict_requires_session_id() {
        let mut v = well_formed_object();
        v.as_object_mut().unwrap().remove("session_id");
        let err = validate_top_level(&v).unwrap_err();
        assert!(matches!(err, CompatError::MissingField("session_id")));
    }

    #[test]
    fn role_mapping_covers_common_external_dialects() {
        assert_eq!(map_external_role("user").unwrap(), HarnessActor::HumanUser);
        assert_eq!(map_external_role("Human").unwrap(), HarnessActor::HumanUser);
        assert_eq!(
            map_external_role("assistant").unwrap(),
            HarnessActor::AgentAssistant
        );
        assert_eq!(
            map_external_role("AI").unwrap(),
            HarnessActor::AgentAssistant
        );
        assert_eq!(
            map_external_role("system").unwrap(),
            HarnessActor::SystemPrompt
        );
        assert_eq!(
            map_external_role("developer").unwrap(),
            HarnessActor::SystemPrompt
        );
        assert_eq!(map_external_role("tool").unwrap(), HarnessActor::ToolResult);
        assert_eq!(
            map_external_role("function").unwrap(),
            HarnessActor::ToolResult
        );
    }

    #[test]
    fn role_mapping_quarantines_unknown_roles() {
        let err = map_external_role("supervisor").unwrap_err();
        assert!(matches!(err, CompatError::UnknownRole(_)));
    }

    #[test]
    fn repo_ref_accepts_exact_match() {
        verify_repo_ref(
            "abc123def456abc123def456abc123def456abcd",
            "abc123def456abc123def456abc123def456abcd",
        )
        .unwrap();
    }

    #[test]
    fn repo_ref_accepts_short_prefix_either_way() {
        verify_repo_ref("abc123", "abc123def456").unwrap();
        verify_repo_ref("abc123def456", "abc123").unwrap();
    }

    #[test]
    fn repo_ref_strips_branch_prefix_before_compare() {
        verify_repo_ref("main@abc123", "abc123def").unwrap();
    }

    #[test]
    fn repo_ref_rejects_divergence() {
        let err = verify_repo_ref("abc123", "ffeedd").unwrap_err();
        assert!(matches!(err, CompatError::RepoRefDiverged { .. }));
    }

    #[test]
    fn parse_strict_passes_known_object_through() {
        let bytes = serde_json::to_vec(&well_formed_object()).unwrap();
        let v = parse_strict(&bytes).unwrap();
        assert!(v.is_object());
    }

    #[test]
    fn parse_strict_propagates_unknown_field_as_io_error() {
        let mut v = well_formed_object();
        v.as_object_mut().unwrap().insert("rogue".into(), json!(1));
        let bytes = serde_json::to_vec(&v).unwrap();
        let err = parse_strict(&bytes).unwrap_err();
        assert!(matches!(err, TraceError::Io(_)));
    }
}
