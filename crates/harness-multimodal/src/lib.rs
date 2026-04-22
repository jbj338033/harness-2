// IMPLEMENTS: D-156, D-278, D-279, D-280, D-281, D-282, D-283, D-284, D-285
//! Multimodal attachment types and the safety scaffolding around them.
//!
//! - [`Attachment`] is a first-class atom in the event payload — never
//!   inlined as raw bytes (D-278). Bytes live in the FS-backed
//!   content-addressed store keyed by [`BlobId`] (D-279).
//! - [`DisplayScope`] is the new fourth sandbox axis (D-281). It picks
//!   how aggressive the screen-control tier may be.
//! - [`vpi`] holds the five-layer Visual Prompt Injection defence (D-282)
//!   — `TrustLabel`, OCR isolation, bounding-box check, action confirm,
//!   session-level quarantine. The blocker tier promotion comes from
//!   VPI-Bench's 100% Browser-Use compromise.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use thiserror::Error;

pub mod attachment_index;
pub mod computer_use;
pub mod cost_subcap;
pub mod stt;
pub mod vpi;

pub use attachment_index::{AttachmentIndex, IndexRow};
pub use computer_use::{ComputerAction, MouseButton, ResolutionTier};
pub use cost_subcap::{
    DEFAULT_VISION_TURN_CAP_USD, DetailLevel, SubcapVerdict, VisionSubcap,
    classify as classify_vision_subcap,
};
pub use stt::{SttBackend, SttConfig};
pub use vpi::{TrustLabel, VpiVerdict, classify_screen_capture, quarantine_session};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct BlobId(pub String);

impl BlobId {
    /// Compute the blob id for raw bytes — first 32 hex chars of blake3
    /// keep the on-disk path short while keeping collision risk inside
    /// the same regime as git's short-sha defaults.
    #[must_use]
    pub fn for_bytes(bytes: &[u8]) -> Self {
        let mut h = blake3::Hasher::new();
        h.update(b"harness/blob/v1\n");
        h.update(bytes);
        let hex: String = h.finalize().to_hex().chars().take(32).collect();
        Self(hex)
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Error)]
pub enum BlobError {
    #[error("blob id {0} is not 32 hex chars")]
    BadId(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

/// D-279: lay out the blob path as `<root>/aa/bb/cc/<rest>`. Three two-
/// hex-digit prefix directories cap any single inode at ~16M entries.
pub fn blob_path(root: &Path, id: &BlobId) -> Result<PathBuf, BlobError> {
    let s = &id.0;
    if s.len() < 6 || !s.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(BlobError::BadId(s.clone()));
    }
    Ok(root
        .join(&s[..2])
        .join(&s[2..4])
        .join(&s[4..6])
        .join(&s[6..]))
}

/// Persist `bytes` under `root` and return the blob id. Idempotent — a
/// re-write of identical bytes lands on the same file. Bumps a side-car
/// `.refcount` integer file so a future GC can drop blobs that fall to
/// zero references.
pub fn store_blob(root: &Path, bytes: &[u8]) -> Result<BlobId, BlobError> {
    let id = BlobId::for_bytes(bytes);
    let path = blob_path(root, &id)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if !path.exists() {
        std::fs::write(&path, bytes)?;
    }
    let refcount_path = path.with_extension("refcount");
    let current: u64 = std::fs::read_to_string(&refcount_path)
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0);
    std::fs::write(&refcount_path, (current + 1).to_string())?;
    Ok(id)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttachmentKind {
    Image,
    Audio,
    Video,
    ScreenCapture,
    Code,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Attachment {
    pub kind: AttachmentKind,
    pub blob_id: BlobId,
    pub mime: String,
    pub size_bytes: u64,
    /// Human caption — may be empty. Filled in by the OCR/STT/caption
    /// extractor (D-283 follow-up).
    #[serde(default)]
    pub caption: String,
    /// D-282: explicit trust label. Screen captures default to
    /// [`TrustLabel::ScreenUntrusted`] so the planner treats their text
    /// content as injection bait.
    #[serde(default)]
    pub trust: TrustLabel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DisplayScope {
    /// Screen never accessed.
    None,
    /// Dedicated VM display — the agent can see its own desktop only.
    DedicatedVirtual,
    /// Host primary display. Marked Danger tier (D-281) — only granted
    /// after explicit per-action approval.
    HostPrimary,
}

impl DisplayScope {
    #[must_use]
    pub fn is_dangerous(self) -> bool {
        matches!(self, Self::HostPrimary)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn blob_id_is_32_hex_chars_and_deterministic() {
        let a = BlobId::for_bytes(b"hello");
        let b = BlobId::for_bytes(b"hello");
        assert_eq!(a, b);
        assert_eq!(a.as_str().len(), 32);
        assert!(a.as_str().chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn blob_path_uses_three_level_prefix() {
        let id = BlobId("aabbccddeeff112233445566778899ff".into());
        let p = blob_path(Path::new("/store"), &id).unwrap();
        assert_eq!(
            p,
            PathBuf::from("/store/aa/bb/cc/ddeeff112233445566778899ff")
        );
    }

    #[test]
    fn malformed_blob_id_is_rejected() {
        let id = BlobId("zz".into());
        assert!(blob_path(Path::new("/x"), &id).is_err());
    }

    #[test]
    fn store_blob_round_trips_and_increments_refcount() {
        let dir = TempDir::new().unwrap();
        let id = store_blob(dir.path(), b"hi").unwrap();
        let p = blob_path(dir.path(), &id).unwrap();
        assert!(p.exists());
        let _ = store_blob(dir.path(), b"hi").unwrap();
        let rc: u64 = std::fs::read_to_string(p.with_extension("refcount"))
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        assert_eq!(rc, 2);
    }

    #[test]
    fn screen_capture_attachment_defaults_to_screen_untrusted() {
        let a = Attachment {
            kind: AttachmentKind::ScreenCapture,
            blob_id: BlobId("0".repeat(32)),
            mime: "image/png".into(),
            size_bytes: 0,
            caption: String::new(),
            trust: TrustLabel::default(),
        };
        // Default trust is Untrusted; D-282 specifically wants screen
        // captures classified at the higher tier on construction.
        assert_eq!(classify_screen_capture(&a), TrustLabel::ScreenUntrusted);
    }

    #[test]
    fn display_scope_dangerous_only_for_host_primary() {
        assert!(!DisplayScope::None.is_dangerous());
        assert!(!DisplayScope::DedicatedVirtual.is_dangerous());
        assert!(DisplayScope::HostPrimary.is_dangerous());
    }

    #[test]
    fn display_scope_ordering_is_least_to_most_powerful() {
        assert!(DisplayScope::None < DisplayScope::DedicatedVirtual);
        assert!(DisplayScope::DedicatedVirtual < DisplayScope::HostPrimary);
    }
}
