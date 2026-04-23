// IMPLEMENTS: D-421
//! Pinned governance file list. CI checks each one exists at the
//! repo root before a release ships.

pub const GOVERNANCE_FILES: &[&str] = &[
    "TRADEMARK.md",
    "FORK.md",
    "GOVERNANCE.md",
    "ARCHIVE_POLICY.md",
];

#[must_use]
pub fn governance_file_count() -> usize {
    GOVERNANCE_FILES.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn four_governance_files() {
        assert_eq!(governance_file_count(), 4);
    }

    #[test]
    fn includes_archive_policy() {
        assert!(GOVERNANCE_FILES.contains(&"ARCHIVE_POLICY.md"));
    }
}
