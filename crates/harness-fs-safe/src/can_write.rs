// IMPLEMENTS: D-067
//! `ToolContext::can_write` gate. Three failure modes:
//!  * canonical path escapes the allow-list root,
//!  * canonical path differs from the requested path (symlink),
//!  * requested path contains `..` after canonicalisation (defence
//!    in depth — should not happen, but if it does, refuse).
//!
//! Pure data — the actual filesystem touch lives in the tools
//! crate. This function is the verdict the tests can drive
//! deterministically.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CanWriteVerdict {
    Allow,
    /// Canonical path escapes the allowed root.
    EscapesRoot {
        canonical: PathBuf,
        root: PathBuf,
    },
    /// Canonical path differs from the requested path — symlink hop.
    SymlinkHop {
        requested: PathBuf,
        canonical: PathBuf,
    },
    /// Canonical path contained a `..` segment.
    TraversalSegment {
        canonical: PathBuf,
    },
}

#[must_use]
pub fn evaluate_can_write(requested: &Path, canonical: &Path, root: &Path) -> CanWriteVerdict {
    if !canonical.starts_with(root) {
        return CanWriteVerdict::EscapesRoot {
            canonical: canonical.to_path_buf(),
            root: root.to_path_buf(),
        };
    }
    if canonical
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return CanWriteVerdict::TraversalSegment {
            canonical: canonical.to_path_buf(),
        };
    }
    if requested != canonical {
        return CanWriteVerdict::SymlinkHop {
            requested: requested.to_path_buf(),
            canonical: canonical.to_path_buf(),
        };
    }
    CanWriteVerdict::Allow
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matching_canonical_inside_root_passes() {
        let v = evaluate_can_write(
            Path::new("/work/a.txt"),
            Path::new("/work/a.txt"),
            Path::new("/work"),
        );
        assert!(matches!(v, CanWriteVerdict::Allow));
    }

    #[test]
    fn escapes_root_refused() {
        let v = evaluate_can_write(
            Path::new("/etc/passwd"),
            Path::new("/etc/passwd"),
            Path::new("/work"),
        );
        assert!(matches!(v, CanWriteVerdict::EscapesRoot { .. }));
    }

    #[test]
    fn symlink_hop_refused() {
        let v = evaluate_can_write(
            Path::new("/work/link.txt"),
            Path::new("/work/real.txt"),
            Path::new("/work"),
        );
        assert!(matches!(v, CanWriteVerdict::SymlinkHop { .. }));
    }

    #[test]
    fn parent_dir_segment_refused() {
        let v = evaluate_can_write(
            Path::new("/work/a/../b.txt"),
            Path::new("/work/a/../b.txt"),
            Path::new("/work"),
        );
        assert!(matches!(v, CanWriteVerdict::TraversalSegment { .. }));
    }
}
