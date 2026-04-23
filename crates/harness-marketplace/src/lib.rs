//! Plugin marketplace — WasmExt registry primitives.
//!
//! Each entry pins a `wasm_blake3_hex` so the daemon refuses any
//! download that doesn't hash to the expected digest. The
//! `CapabilityManifest` lists the capabilities the plugin will ask
//! for; the install path requires the user to grant each one before
//! the plugin can run.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SemVer {
    pub major: u16,
    pub minor: u16,
    pub patch: u16,
}

impl SemVer {
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        let parts: Vec<&str> = s.split('.').collect();
        if parts.len() != 3 {
            return None;
        }
        Some(Self {
            major: parts[0].parse().ok()?,
            minor: parts[1].parse().ok()?,
            patch: parts[2].parse().ok()?,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WasmCapability {
    NetHttpsRead,
    FsReadProject,
    FsWriteProject,
    Tools,
    MemoryRead,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityManifest {
    pub requested: BTreeSet<WasmCapability>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WasmExtEntry {
    pub plugin_id: String,
    pub version: SemVer,
    pub wasm_blake3_hex: String,
    pub publisher: String,
    pub manifest: CapabilityManifest,
    pub yanked: bool,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum InstallError {
    #[error("plugin {0} version {1:?} is yanked")]
    Yanked(String, SemVer),
    #[error("hash mismatch: expected {expected}, got {actual}")]
    HashMismatch { expected: String, actual: String },
    #[error("capability {0:?} not granted")]
    CapabilityNotGranted(WasmCapability),
    #[error("plugin id is empty")]
    EmptyPluginId,
    #[error("publisher is empty")]
    EmptyPublisher,
}

pub fn evaluate_install(
    entry: &WasmExtEntry,
    downloaded_bytes: &[u8],
    granted: &BTreeSet<WasmCapability>,
) -> Result<(), InstallError> {
    if entry.plugin_id.trim().is_empty() {
        return Err(InstallError::EmptyPluginId);
    }
    if entry.publisher.trim().is_empty() {
        return Err(InstallError::EmptyPublisher);
    }
    if entry.yanked {
        return Err(InstallError::Yanked(entry.plugin_id.clone(), entry.version));
    }
    let mut h = blake3::Hasher::new();
    h.update(b"harness/marketplace/wasm/v1\n");
    h.update(downloaded_bytes);
    let actual = h.finalize().to_hex().to_string();
    if actual != entry.wasm_blake3_hex {
        return Err(InstallError::HashMismatch {
            expected: entry.wasm_blake3_hex.clone(),
            actual,
        });
    }
    for cap in &entry.manifest.requested {
        if !granted.contains(cap) {
            return Err(InstallError::CapabilityNotGranted(*cap));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(bytes: &[u8], yanked: bool) -> WasmExtEntry {
        let mut h = blake3::Hasher::new();
        h.update(b"harness/marketplace/wasm/v1\n");
        h.update(bytes);
        let digest = h.finalize().to_hex().to_string();
        let mut requested = BTreeSet::new();
        requested.insert(WasmCapability::FsReadProject);
        WasmExtEntry {
            plugin_id: "p1".into(),
            version: SemVer::parse("1.2.3").unwrap(),
            wasm_blake3_hex: digest,
            publisher: "alice".into(),
            manifest: CapabilityManifest { requested },
            yanked,
        }
    }

    fn granted(caps: &[WasmCapability]) -> BTreeSet<WasmCapability> {
        caps.iter().copied().collect()
    }

    #[test]
    fn semver_parse_round_trip() {
        let v = SemVer::parse("1.2.3").unwrap();
        assert_eq!(v.major, 1);
        assert_eq!(v.minor, 2);
        assert_eq!(v.patch, 3);
        assert!(SemVer::parse("1.2").is_none());
        assert!(SemVer::parse("a.b.c").is_none());
    }

    #[test]
    fn full_install_passes() {
        let bytes = b"wasm";
        assert!(
            evaluate_install(
                &entry(bytes, false),
                bytes,
                &granted(&[WasmCapability::FsReadProject]),
            )
            .is_ok()
        );
    }

    #[test]
    fn yanked_refused() {
        let bytes = b"wasm";
        let r = evaluate_install(
            &entry(bytes, true),
            bytes,
            &granted(&[WasmCapability::FsReadProject]),
        );
        assert!(matches!(r, Err(InstallError::Yanked(_, _))));
    }

    #[test]
    fn hash_mismatch_refused() {
        let bytes = b"wasm";
        let r = evaluate_install(
            &entry(bytes, false),
            b"different",
            &granted(&[WasmCapability::FsReadProject]),
        );
        assert!(matches!(r, Err(InstallError::HashMismatch { .. })));
    }

    #[test]
    fn missing_capability_refused() {
        let bytes = b"wasm";
        let r = evaluate_install(&entry(bytes, false), bytes, &granted(&[]));
        assert!(matches!(r, Err(InstallError::CapabilityNotGranted(_))));
    }

    #[test]
    fn empty_publisher_refused() {
        let bytes = b"wasm";
        let mut e = entry(bytes, false);
        e.publisher = "  ".into();
        let r = evaluate_install(&e, bytes, &granted(&[WasmCapability::FsReadProject]));
        assert!(matches!(r, Err(InstallError::EmptyPublisher)));
    }
}
