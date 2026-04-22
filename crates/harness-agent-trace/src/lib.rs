// IMPLEMENTS: D-148, D-161, D-169, D-170, D-177, D-318, D-430
pub mod archive;
pub mod compat;
pub mod conversation_url;
pub mod git_sidecar;
mod keyring;
pub mod retention;

pub use keyring::{KeyId, Keyring, key_id_for_public};

use harness_auth::{PrivateKey, PublicKey, SignatureBytes, generate_keypair};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};
use thiserror::Error;

pub const CURRENT_SPEC_VERSION: &str = "trace/0.2";
pub const MIN_SUPPORTED_SPEC: &str = "trace/0.1";

const TRACE_KEY_FILENAME: &str = "trace-key";

#[derive(Debug, Error)]
pub enum TraceError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("signature verification failed")]
    BadSignature,
    #[error("unsupported spec_version {actual}; minimum is {min}")]
    UnsupportedSpec { actual: String, min: String },
    #[error("session_id {0} collides with an existing trace")]
    SessionCollision(String),
    #[error("trace key file at {0} is corrupt: {1}")]
    BadKeyFile(PathBuf, String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceFile {
    pub spec_version: String,
    pub session_id: String,
    pub payload: Value,
    pub public_key: PublicKey,
    pub signature: SignatureBytes,
    /// 16-byte (32 hex) deterministic id of the writer pubkey. Lets a
    /// reader pick the right active key when several have rotated through
    /// the trust store. D-177b pinned the size at 16 bytes — 32 bits of
    /// collision resistance was deemed too thin for ~30 year horizons.
    #[serde(default)]
    pub key_id: String,
}

/// `<untrusted source="trace">` envelope per D-152a — every importer must
/// keep this wrapper intact when feeding the body into a prompt or planner.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UntrustedTrace {
    pub spec_version: String,
    pub session_id: String,
    pub payload: Value,
    pub source_label: &'static str,
}

pub fn key_path(dir: &Path) -> PathBuf {
    dir.join(TRACE_KEY_FILENAME)
}

pub fn load_or_generate_key(dir: &Path) -> Result<(PrivateKey, PublicKey), TraceError> {
    std::fs::create_dir_all(dir)?;
    let path = key_path(dir);
    if path.exists() {
        let raw = std::fs::read(&path)?;
        let bytes: [u8; 32] = raw
            .as_slice()
            .try_into()
            .map_err(|_| TraceError::BadKeyFile(path.clone(), "expected 32 raw bytes".into()))?;
        let sk = PrivateKey::from_bytes(&bytes);
        let pk = sk.public();
        return Ok((sk, pk));
    }
    let (sk, pk) = generate_keypair();
    let bytes = sk.to_bytes();
    std::fs::write(&path, bytes)?;
    set_owner_only(&path)?;
    Ok((sk, pk))
}

pub fn export(
    sk: &PrivateKey,
    spec_version: impl Into<String>,
    session_id: impl Into<String>,
    payload: Value,
    out: &Path,
) -> Result<(), TraceError> {
    let spec_version = spec_version.into();
    let session_id = session_id.into();
    let canon_payload = canonical_bytes(&spec_version, &session_id, &payload)?;
    let signature = sk.sign(&canon_payload);
    let public_key = sk.public();
    let key_id = key_id_for_public(&public_key).0;
    let file = TraceFile {
        spec_version,
        session_id,
        payload,
        public_key,
        signature,
        key_id,
    };
    let bytes = serde_json::to_vec_pretty(&file)?;
    std::fs::write(out, bytes)?;
    set_owner_only(out)?;
    Ok(())
}

pub fn import(path: &Path) -> Result<UntrustedTrace, TraceError> {
    let bytes = std::fs::read(path)?;
    import_bytes(&bytes)
}

pub fn import_bytes(bytes: &[u8]) -> Result<UntrustedTrace, TraceError> {
    let file: TraceFile = serde_json::from_slice(bytes)?;
    if !is_spec_supported(&file.spec_version) {
        return Err(TraceError::UnsupportedSpec {
            actual: file.spec_version.clone(),
            min: MIN_SUPPORTED_SPEC.into(),
        });
    }
    let canon = canonical_bytes(&file.spec_version, &file.session_id, &file.payload)?;
    file.public_key
        .verify(&canon, &file.signature)
        .map_err(|_| TraceError::BadSignature)?;
    Ok(UntrustedTrace {
        spec_version: file.spec_version,
        session_id: file.session_id,
        payload: file.payload,
        source_label: "trace",
    })
}

pub fn import_into_dir(
    src: &Path,
    dst_dir: &Path,
    rename_on_collision: bool,
) -> Result<(UntrustedTrace, PathBuf), TraceError> {
    let trace = import(src)?;
    let target = next_free_path(dst_dir, &trace.session_id, rename_on_collision)?;
    std::fs::create_dir_all(dst_dir)?;
    std::fs::copy(src, &target)?;
    set_owner_only(&target)?;
    Ok((trace, target))
}

fn next_free_path(
    dir: &Path,
    session_id: &str,
    rename_on_collision: bool,
) -> Result<PathBuf, TraceError> {
    let primary = dir.join(format!("{session_id}.trace.json"));
    if !primary.exists() {
        return Ok(primary);
    }
    if !rename_on_collision {
        return Err(TraceError::SessionCollision(session_id.into()));
    }
    for n in 1..1000 {
        let candidate = dir.join(format!("{session_id}.{n}.trace.json"));
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    Err(TraceError::SessionCollision(format!(
        "{session_id} (exhausted rename attempts)"
    )))
}

fn canonical_bytes(
    spec_version: &str,
    session_id: &str,
    payload: &Value,
) -> Result<Vec<u8>, TraceError> {
    let canon = serde_json::json!({
        "spec_version": spec_version,
        "session_id": session_id,
        "payload": payload,
    });
    Ok(serde_json::to_vec(&canon)?)
}

fn is_spec_supported(actual: &str) -> bool {
    actual >= MIN_SUPPORTED_SPEC
}

fn set_owner_only(path: &Path) -> Result<(), TraceError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(path)?.permissions();
        perms.set_mode(0o600);
        std::fs::set_permissions(path, perms)?;
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::TempDir;

    fn temp_dir() -> TempDir {
        TempDir::new().unwrap()
    }

    #[test]
    fn key_lifecycle_creates_file_with_0600() {
        let dir = temp_dir();
        let (_sk1, pk1) = load_or_generate_key(dir.path()).unwrap();
        let (_sk2, pk2) = load_or_generate_key(dir.path()).unwrap();
        assert_eq!(pk1, pk2, "second call must reuse the on-disk key");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::metadata(key_path(dir.path()))
                .unwrap()
                .permissions();
            assert_eq!(perms.mode() & 0o777, 0o600, "trace-key must be owner-only");
        }
    }

    #[test]
    fn export_and_import_roundtrip() {
        let dir = temp_dir();
        let (sk, _pk) = load_or_generate_key(dir.path()).unwrap();
        let out = dir.path().join("trace.json");
        export(
            &sk,
            CURRENT_SPEC_VERSION,
            "session-a",
            json!({"events": [1, 2, 3]}),
            &out,
        )
        .unwrap();

        let imported = import(&out).unwrap();
        assert_eq!(imported.spec_version, CURRENT_SPEC_VERSION);
        assert_eq!(imported.session_id, "session-a");
        assert_eq!(imported.source_label, "trace");
        assert_eq!(imported.payload, json!({"events": [1, 2, 3]}));
    }

    #[test]
    fn import_rejects_tampered_payload() {
        let dir = temp_dir();
        let (sk, _) = load_or_generate_key(dir.path()).unwrap();
        let out = dir.path().join("trace.json");
        export(&sk, CURRENT_SPEC_VERSION, "s", json!({"k": 1}), &out).unwrap();

        // Tamper: rewrite the payload field while keeping the signature.
        let mut file: TraceFile = serde_json::from_slice(&std::fs::read(&out).unwrap()).unwrap();
        file.payload = json!({"k": 2});
        std::fs::write(&out, serde_json::to_vec(&file).unwrap()).unwrap();

        let err = import(&out).unwrap_err();
        assert!(matches!(err, TraceError::BadSignature), "got {err:?}");
    }

    #[test]
    fn import_rejects_unsupported_spec() {
        let dir = temp_dir();
        let (sk, _) = load_or_generate_key(dir.path()).unwrap();
        let out = dir.path().join("trace.json");
        // Manually craft a TraceFile with an old spec — sign it correctly so
        // we know it's the version check (not the signature) that fails.
        let payload = json!({});
        let canon = canonical_bytes("trace/0.0", "s", &payload).unwrap();
        let signature = sk.sign(&canon);
        let pk = sk.public();
        let key_id = key_id_for_public(&pk).0;
        let file = TraceFile {
            spec_version: "trace/0.0".into(),
            session_id: "s".into(),
            payload,
            public_key: pk,
            signature,
            key_id,
        };
        std::fs::write(&out, serde_json::to_vec(&file).unwrap()).unwrap();
        let err = import(&out).unwrap_err();
        assert!(
            matches!(err, TraceError::UnsupportedSpec { .. }),
            "got {err:?}"
        );
    }

    #[test]
    fn import_into_dir_renames_on_collision_when_allowed() {
        let dir = temp_dir();
        let (sk, _) = load_or_generate_key(dir.path()).unwrap();
        let src = dir.path().join("src.json");
        export(&sk, CURRENT_SPEC_VERSION, "sess", json!({}), &src).unwrap();

        let store = temp_dir();
        let (_t1, p1) = import_into_dir(&src, store.path(), true).unwrap();
        assert!(p1.ends_with("sess.trace.json"));
        let (_t2, p2) = import_into_dir(&src, store.path(), true).unwrap();
        assert!(p2.ends_with("sess.1.trace.json"));
    }

    #[test]
    fn import_into_dir_refuses_collision_without_rename() {
        let dir = temp_dir();
        let (sk, _) = load_or_generate_key(dir.path()).unwrap();
        let src = dir.path().join("src.json");
        export(&sk, CURRENT_SPEC_VERSION, "sess", json!({}), &src).unwrap();

        let store = temp_dir();
        import_into_dir(&src, store.path(), false).unwrap();
        let err = import_into_dir(&src, store.path(), false).unwrap_err();
        assert!(
            matches!(err, TraceError::SessionCollision(_)),
            "got {err:?}"
        );
    }

    #[test]
    fn exported_trace_carries_matching_key_id() {
        let dir = temp_dir();
        let (sk, pk) = load_or_generate_key(dir.path()).unwrap();
        let out = dir.path().join("trace.json");
        export(&sk, CURRENT_SPEC_VERSION, "kid-test", json!({}), &out).unwrap();
        let bytes = std::fs::read(&out).unwrap();
        let file: TraceFile = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(file.key_id, key_id_for_public(&pk).0);
        assert_eq!(file.key_id.len(), 32);
    }

    #[test]
    fn untrusted_label_is_constant() {
        let dir = temp_dir();
        let (sk, _) = load_or_generate_key(dir.path()).unwrap();
        let out = dir.path().join("t.json");
        export(&sk, CURRENT_SPEC_VERSION, "x", json!({}), &out).unwrap();
        let t = import(&out).unwrap();
        assert_eq!(t.source_label, "trace");
    }
}
