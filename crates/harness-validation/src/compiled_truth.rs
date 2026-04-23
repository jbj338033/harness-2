// IMPLEMENTS: D-192
//! External `compiled_truth` verifier (monthly cadence). The
//! manifest publishes blake3 digests of the canonical compiled
//! artifacts; the verifier downloads each artifact, recomputes the
//! digest, and compares.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompiledTruthManifest {
    pub version: String,
    /// artifact path → expected blake3 hex digest.
    pub artifacts: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompiledTruthVerifyOutcome {
    Pass,
    /// Listed artifact paths whose digest didn't match.
    Fail(Vec<String>),
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CompiledTruthVerifyError {
    #[error("manifest is empty")]
    EmptyManifest,
}

/// `actual` is path → bytes for each declared artifact. Missing
/// artifacts are reported as failures, not as a separate error,
/// because operationally that's the same problem.
pub fn verify_truth(
    manifest: &CompiledTruthManifest,
    actual: &BTreeMap<String, Vec<u8>>,
) -> Result<CompiledTruthVerifyOutcome, CompiledTruthVerifyError> {
    if manifest.artifacts.is_empty() {
        return Err(CompiledTruthVerifyError::EmptyManifest);
    }
    let mut bad: Vec<String> = Vec::new();
    for (path, expected_hex) in &manifest.artifacts {
        let Some(bytes) = actual.get(path) else {
            bad.push(path.clone());
            continue;
        };
        let mut h = blake3::Hasher::new();
        h.update(bytes);
        let actual_hex = h.finalize().to_hex().to_string();
        if &actual_hex != expected_hex {
            bad.push(path.clone());
        }
    }
    if bad.is_empty() {
        Ok(CompiledTruthVerifyOutcome::Pass)
    } else {
        Ok(CompiledTruthVerifyOutcome::Fail(bad))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn digest(bytes: &[u8]) -> String {
        let mut h = blake3::Hasher::new();
        h.update(bytes);
        h.finalize().to_hex().to_string()
    }

    fn manifest(entries: &[(&str, &[u8])]) -> CompiledTruthManifest {
        let mut artifacts = BTreeMap::new();
        for (path, bytes) in entries {
            artifacts.insert((*path).to_string(), digest(bytes));
        }
        CompiledTruthManifest {
            version: "v1".into(),
            artifacts,
        }
    }

    #[test]
    fn empty_manifest_errors() {
        let m = CompiledTruthManifest {
            version: "v1".into(),
            artifacts: BTreeMap::new(),
        };
        assert!(verify_truth(&m, &BTreeMap::new()).is_err());
    }

    #[test]
    fn matching_digest_passes() {
        let m = manifest(&[("a.bin", b"hello")]);
        let mut actual = BTreeMap::new();
        actual.insert("a.bin".into(), b"hello".to_vec());
        assert_eq!(
            verify_truth(&m, &actual).unwrap(),
            CompiledTruthVerifyOutcome::Pass
        );
    }

    #[test]
    fn mismatching_digest_fails_with_path() {
        let m = manifest(&[("a.bin", b"hello")]);
        let mut actual = BTreeMap::new();
        actual.insert("a.bin".into(), b"world".to_vec());
        match verify_truth(&m, &actual).unwrap() {
            CompiledTruthVerifyOutcome::Fail(paths) => assert_eq!(paths, vec!["a.bin"]),
            CompiledTruthVerifyOutcome::Pass => panic!("expected fail"),
        }
    }

    #[test]
    fn missing_artifact_reports_path() {
        let m = manifest(&[("a.bin", b"x")]);
        let actual = BTreeMap::new();
        match verify_truth(&m, &actual).unwrap() {
            CompiledTruthVerifyOutcome::Fail(paths) => assert_eq!(paths, vec!["a.bin"]),
            CompiledTruthVerifyOutcome::Pass => panic!("expected fail"),
        }
    }
}
