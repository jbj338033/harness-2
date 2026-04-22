// IMPLEMENTS: D-430
//! 90-day default retention sweep — Armilla + Chaucer 2025-04 AI liability
//! policies treat anything older than ~3 months as out-of-scope evidence,
//! so we drop traces past that age unless the user pins them with the
//! `keep` lockfile.

use crate::TraceError;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

pub const DEFAULT_RETENTION_DAYS: u64 = 90;
const KEEP_LOCKFILE: &str = ".keep";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PruneStats {
    pub considered: usize,
    pub pruned: usize,
    pub kept: usize,
}

/// Delete `*.json` files inside `dir` that are older than `max_age`. Files
/// in a subdirectory containing a `.keep` marker are skipped — that lets a
/// reviewer pin a session as evidence without flipping a global flag.
pub fn prune_older_than(dir: &Path, max_age: Duration) -> Result<PruneStats, TraceError> {
    let mut stats = PruneStats::default();
    if !dir.exists() {
        return Ok(stats);
    }
    let cutoff = SystemTime::now()
        .checked_sub(max_age)
        .unwrap_or(SystemTime::UNIX_EPOCH);
    sweep(dir, cutoff, &mut stats)?;
    Ok(stats)
}

fn sweep(dir: &Path, cutoff: SystemTime, stats: &mut PruneStats) -> Result<(), TraceError> {
    if dir.join(KEEP_LOCKFILE).exists() {
        // Honour the lockfile: count children but never delete.
        for entry in std::fs::read_dir(dir)? {
            let path = entry?.path();
            if is_trace_file(&path) {
                stats.considered += 1;
                stats.kept += 1;
            } else if path.is_dir() {
                sweep(&path, cutoff, stats)?;
            }
        }
        return Ok(());
    }
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            sweep(&path, cutoff, stats)?;
            continue;
        }
        if !is_trace_file(&path) {
            continue;
        }
        stats.considered += 1;
        let modified = std::fs::metadata(&path)?.modified()?;
        if modified < cutoff {
            std::fs::remove_file(&path)?;
            stats.pruned += 1;
        } else {
            stats.kept += 1;
        }
    }
    Ok(())
}

fn is_trace_file(path: &Path) -> bool {
    path.extension().and_then(|s| s.to_str()) == Some("json")
}

/// Convenience wrapper for the default 90-day window.
pub fn prune_default(dir: &Path) -> Result<PruneStats, TraceError> {
    prune_older_than(dir, Duration::from_secs(DEFAULT_RETENTION_DAYS * 86_400))
}

#[must_use]
pub fn keep_lockfile_path(dir: &Path) -> PathBuf {
    dir.join(KEEP_LOCKFILE)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn touch(dir: &Path, name: &str) -> PathBuf {
        let p = dir.join(name);
        std::fs::write(&p, b"{}").unwrap();
        p
    }

    fn backdate(path: &Path, age: Duration) {
        let when = SystemTime::now() - age;
        let f = std::fs::OpenOptions::new().write(true).open(path).unwrap();
        f.set_modified(when).unwrap();
    }

    #[test]
    fn prune_drops_only_old_json_files() {
        let dir = TempDir::new().unwrap();
        let new = touch(dir.path(), "fresh.json");
        let old = touch(dir.path(), "stale.json");
        backdate(&old, Duration::from_secs(91 * 86_400));

        let stats = prune_default(dir.path()).unwrap();
        assert_eq!(stats.considered, 2);
        assert_eq!(stats.pruned, 1);
        assert_eq!(stats.kept, 1);
        assert!(new.exists());
        assert!(!old.exists());
    }

    #[test]
    fn non_json_files_are_ignored() {
        let dir = TempDir::new().unwrap();
        let other = touch(dir.path(), "note.txt");
        backdate(&other, Duration::from_secs(365 * 86_400));
        let stats = prune_default(dir.path()).unwrap();
        assert_eq!(stats.pruned, 0);
        assert!(other.exists());
    }

    #[test]
    fn keep_lockfile_pins_directory() {
        let dir = TempDir::new().unwrap();
        let pinned = touch(dir.path(), "evidence.json");
        backdate(&pinned, Duration::from_secs(180 * 86_400));
        std::fs::write(keep_lockfile_path(dir.path()), b"do not delete").unwrap();
        let stats = prune_default(dir.path()).unwrap();
        assert_eq!(stats.pruned, 0);
        assert_eq!(stats.kept, 1);
        assert!(pinned.exists());
    }

    #[test]
    fn prune_recurses_into_subdirs() {
        let dir = TempDir::new().unwrap();
        let sub = dir.path().join("nested");
        std::fs::create_dir(&sub).unwrap();
        let old = touch(&sub, "old.json");
        backdate(&old, Duration::from_secs(120 * 86_400));
        let stats = prune_default(dir.path()).unwrap();
        assert_eq!(stats.pruned, 1);
        assert!(!old.exists());
    }

    #[test]
    fn prune_returns_zero_for_missing_dir() {
        let stats = prune_default(Path::new("/this/does/not/exist/qq")).unwrap();
        assert_eq!(stats, PruneStats::default());
    }

    #[test]
    fn custom_window_can_be_short() {
        let dir = TempDir::new().unwrap();
        let p = touch(dir.path(), "x.json");
        backdate(&p, Duration::from_secs(60));
        let stats = prune_older_than(dir.path(), Duration::from_secs(30)).unwrap();
        assert_eq!(stats.pruned, 1);
    }
}
