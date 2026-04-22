// IMPLEMENTS: D-430
//! Insurance / legal export bundler — gathers signed traces into a single
//! manifest a regulator or carrier can ingest. Each entry verifies before
//! it ships so a tampered file is dropped rather than handed off as-is.

use crate::{TraceError, TraceFile};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportManifest {
    pub generated_at_ms: i64,
    pub source_dir: PathBuf,
    pub spec_version: String,
    pub entries: Vec<ExportEntry>,
    #[serde(default)]
    pub skipped: Vec<SkippedEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportEntry {
    pub path: PathBuf,
    pub session_id: String,
    pub spec_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkippedEntry {
    pub path: PathBuf,
    pub reason: String,
}

/// Walk `source_dir`, verify each `*.json` trace, and write a manifest +
/// the verified traces to `out_dir`. Files that fail verification are not
/// copied; their paths are listed under `skipped` for the audit trail.
pub fn export_archive(
    source_dir: &Path,
    out_dir: &Path,
    spec_label: impl Into<String>,
) -> Result<ExportManifest, TraceError> {
    std::fs::create_dir_all(out_dir)?;
    let spec_version = spec_label.into();
    let mut entries = Vec::new();
    let mut skipped = Vec::new();
    if source_dir.exists() {
        walk(source_dir, source_dir, out_dir, &mut entries, &mut skipped)?;
    }
    let manifest = ExportManifest {
        generated_at_ms: now_ms(),
        source_dir: source_dir.to_path_buf(),
        spec_version,
        entries,
        skipped,
    };
    let manifest_path = out_dir.join("manifest.json");
    let bytes = serde_json::to_vec_pretty(&manifest)?;
    std::fs::write(&manifest_path, bytes)?;
    Ok(manifest)
}

fn walk(
    root: &Path,
    dir: &Path,
    out_dir: &Path,
    entries: &mut Vec<ExportEntry>,
    skipped: &mut Vec<SkippedEntry>,
) -> Result<(), TraceError> {
    for raw in std::fs::read_dir(dir)? {
        let path = raw?.path();
        if path.is_dir() {
            walk(root, &path, out_dir, entries, skipped)?;
            continue;
        }
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        match crate::import(&path) {
            Ok(trace) => {
                let rel = path.strip_prefix(root).unwrap_or(&path);
                let dest = out_dir.join(rel);
                if let Some(parent) = dest.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::copy(&path, &dest)?;
                entries.push(ExportEntry {
                    path: rel.to_path_buf(),
                    session_id: trace.session_id,
                    spec_version: trace.spec_version,
                });
            }
            Err(e) => {
                // Couldn't parse / verify — record but don't propagate, so
                // a single tamper doesn't void the whole archive.
                let reason = e.to_string();
                let json_err =
                    serde_json::from_slice::<TraceFile>(&std::fs::read(&path).unwrap_or_default());
                let path_rel = path.strip_prefix(root).unwrap_or(&path).to_path_buf();
                skipped.push(SkippedEntry {
                    path: path_rel,
                    reason: if json_err.is_err() {
                        format!("malformed json: {reason}")
                    } else {
                        reason
                    },
                });
            }
        }
    }
    Ok(())
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_secs(0))
        .as_millis()
        .try_into()
        .unwrap_or(i64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CURRENT_SPEC_VERSION, export, load_or_generate_key};
    use serde_json::json;
    use tempfile::TempDir;

    #[test]
    fn export_copies_verified_traces() {
        let key_dir = TempDir::new().unwrap();
        let (sk, _pk) = load_or_generate_key(key_dir.path()).unwrap();
        let src = TempDir::new().unwrap();
        export(
            &sk,
            CURRENT_SPEC_VERSION,
            "session-1",
            json!({"x": 1}),
            &src.path().join("a.json"),
        )
        .unwrap();
        export(
            &sk,
            CURRENT_SPEC_VERSION,
            "session-2",
            json!({"x": 2}),
            &src.path().join("b.json"),
        )
        .unwrap();

        let out = TempDir::new().unwrap();
        let manifest = export_archive(src.path(), out.path(), "harness-1.0").unwrap();
        assert_eq!(manifest.entries.len(), 2);
        assert!(manifest.skipped.is_empty());
        assert!(out.path().join("a.json").exists());
        assert!(out.path().join("b.json").exists());
        assert!(out.path().join("manifest.json").exists());
    }

    #[test]
    fn export_lists_tampered_files_in_skipped() {
        let key_dir = TempDir::new().unwrap();
        let (sk, _pk) = load_or_generate_key(key_dir.path()).unwrap();
        let src = TempDir::new().unwrap();
        let target = src.path().join("c.json");
        export(
            &sk,
            CURRENT_SPEC_VERSION,
            "session",
            json!({"x": 1}),
            &target,
        )
        .unwrap();

        // Tamper: re-write payload after signing.
        let mut file: TraceFile = serde_json::from_slice(&std::fs::read(&target).unwrap()).unwrap();
        file.payload = json!({"x": 99});
        std::fs::write(&target, serde_json::to_vec(&file).unwrap()).unwrap();

        let out = TempDir::new().unwrap();
        let manifest = export_archive(src.path(), out.path(), "harness-1.0").unwrap();
        assert!(manifest.entries.is_empty());
        assert_eq!(manifest.skipped.len(), 1);
        assert!(!out.path().join("c.json").exists());
    }

    #[test]
    fn manifest_has_generation_timestamp_and_spec() {
        let src = TempDir::new().unwrap();
        let out = TempDir::new().unwrap();
        let manifest = export_archive(src.path(), out.path(), "v2").unwrap();
        assert_eq!(manifest.spec_version, "v2");
        assert!(manifest.generated_at_ms > 0);
    }

    #[test]
    fn export_handles_missing_source_dir() {
        let out = TempDir::new().unwrap();
        let manifest = export_archive(Path::new("/no/such/path/xyz"), out.path(), "v1").unwrap();
        assert!(manifest.entries.is_empty());
        assert!(out.path().join("manifest.json").exists());
    }
}
