// IMPLEMENTS: D-318
//! Sidecar writer per the Cursor + Cognition Agent Trace RFC: every trace
//! lives at `.git/agent-trace/<commit-sha>.json`, attaching the run that
//! produced the change to the change itself. `git fetch` ships the sidecar
//! alongside the commit by virtue of being inside `.git/`.

use crate::{TraceError, TraceFile};
use harness_auth::PrivateKey;
use serde_json::Value;
use std::path::{Path, PathBuf};

const SIDECAR_DIRNAME: &str = "agent-trace";

#[must_use]
pub fn sidecar_dir(repo_root: &Path) -> PathBuf {
    repo_root.join(".git").join(SIDECAR_DIRNAME)
}

#[must_use]
pub fn sidecar_path(repo_root: &Path, commit_sha: &str) -> PathBuf {
    sidecar_dir(repo_root).join(format!("{commit_sha}.json"))
}

/// Validate the SHA is hex of the right length so a malicious caller can't
/// traverse out of the sidecar dir via a forged commit ref.
fn is_well_formed_sha(sha: &str) -> bool {
    matches!(sha.len(), 7..=64) && sha.chars().all(|c| c.is_ascii_hexdigit())
}

/// Write a signed trace file into `<repo>/.git/agent-trace/<sha>.json`.
///
/// # Errors
/// Returns [`TraceError::Io`] if the sha is malformed or the directory
/// cannot be created / written. Owner-only file mode is set on unix.
pub fn write_sidecar(
    repo_root: &Path,
    commit_sha: &str,
    sk: &PrivateKey,
    spec_version: impl Into<String>,
    session_id: impl Into<String>,
    payload: Value,
) -> Result<PathBuf, TraceError> {
    if !is_well_formed_sha(commit_sha) {
        return Err(TraceError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("commit sha {commit_sha} is not hex"),
        )));
    }
    let dir = sidecar_dir(repo_root);
    std::fs::create_dir_all(&dir)?;
    let target = dir.join(format!("{commit_sha}.json"));
    crate::export(sk, spec_version, session_id, payload, &target)?;
    Ok(target)
}

/// List every sidecar trace below `<repo>/.git/agent-trace/`.
pub fn list_sidecars(repo_root: &Path) -> Result<Vec<PathBuf>, TraceError> {
    let dir = sidecar_dir(repo_root);
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for entry in std::fs::read_dir(&dir)? {
        let path = entry?.path();
        if path.extension().and_then(|s| s.to_str()) == Some("json") {
            out.push(path);
        }
    }
    out.sort();
    Ok(out)
}

/// Read a sidecar by commit sha. Returns the verified trace plus the path.
pub fn read_sidecar(
    repo_root: &Path,
    commit_sha: &str,
) -> Result<(crate::UntrustedTrace, PathBuf), TraceError> {
    let path = sidecar_path(repo_root, commit_sha);
    let trace = crate::import(&path)?;
    Ok((trace, path))
}

/// Inspect the bytes-on-disk to see what the sidecar pins as `session_id`
/// without first verifying the signature — useful for tooling that needs
/// to triage many sidecars cheaply (still parses JSON, doesn't trust).
pub fn peek_session_id(path: &Path) -> Result<String, TraceError> {
    let bytes = std::fs::read(path)?;
    let file: TraceFile = serde_json::from_slice(&bytes)?;
    Ok(file.session_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CURRENT_SPEC_VERSION, load_or_generate_key};
    use serde_json::json;
    use tempfile::TempDir;

    fn fake_repo() -> TempDir {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join(".git")).unwrap();
        dir
    }

    #[test]
    fn sidecar_dir_resolves_to_dot_git() {
        let dir = fake_repo();
        assert_eq!(
            sidecar_dir(dir.path()),
            dir.path().join(".git").join("agent-trace")
        );
    }

    #[test]
    fn write_then_read_round_trips() {
        let repo = fake_repo();
        let key_dir = TempDir::new().unwrap();
        let (sk, _pk) = load_or_generate_key(key_dir.path()).unwrap();
        let sha = "deadbeefcafe1234deadbeefcafe1234deadbeef";
        let path = write_sidecar(
            repo.path(),
            sha,
            &sk,
            CURRENT_SPEC_VERSION,
            "session-x",
            json!({"hello": "world"}),
        )
        .unwrap();
        assert!(path.ends_with(format!(".git/agent-trace/{sha}.json")));
        let (trace, _p) = read_sidecar(repo.path(), sha).unwrap();
        assert_eq!(trace.session_id, "session-x");
        assert_eq!(trace.payload, json!({"hello": "world"}));
    }

    #[test]
    fn rejects_malformed_sha() {
        let repo = fake_repo();
        let key_dir = TempDir::new().unwrap();
        let (sk, _pk) = load_or_generate_key(key_dir.path()).unwrap();
        let err = write_sidecar(
            repo.path(),
            "../escape",
            &sk,
            CURRENT_SPEC_VERSION,
            "s",
            json!({}),
        )
        .unwrap_err();
        assert!(matches!(err, TraceError::Io(_)), "got {err:?}");
    }

    #[test]
    fn list_returns_sorted_jsons_only() {
        let repo = fake_repo();
        let key_dir = TempDir::new().unwrap();
        let (sk, _pk) = load_or_generate_key(key_dir.path()).unwrap();
        for sha in ["aaaaaaa", "bbbbbbb", "ccccccc"] {
            write_sidecar(repo.path(), sha, &sk, CURRENT_SPEC_VERSION, sha, json!({})).unwrap();
        }
        // drop a non-json file that must be ignored
        std::fs::write(sidecar_dir(repo.path()).join("note.txt"), "ignore me").unwrap();
        let listed = list_sidecars(repo.path()).unwrap();
        assert_eq!(listed.len(), 3);
        assert!(listed[0] < listed[1] && listed[1] < listed[2]);
    }

    #[test]
    fn list_returns_empty_when_dir_absent() {
        let repo = fake_repo();
        assert!(list_sidecars(repo.path()).unwrap().is_empty());
    }

    #[test]
    fn peek_extracts_session_without_verifying_sig() {
        let repo = fake_repo();
        let key_dir = TempDir::new().unwrap();
        let (sk, _pk) = load_or_generate_key(key_dir.path()).unwrap();
        let sha = "1234567890abcdef";
        let path = write_sidecar(
            repo.path(),
            sha,
            &sk,
            CURRENT_SPEC_VERSION,
            "peeked",
            json!({}),
        )
        .unwrap();
        assert_eq!(peek_session_id(&path).unwrap(), "peeked");
    }
}
