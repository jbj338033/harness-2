// IMPLEMENTS: D-113
//! Per-platform safe-open spec. The `harness-tools-fs` crate
//! implements the actual binding behind `#[cfg]` gates; this module
//! is the data-only spec the workspace tests check against.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Platform {
    Linux,
    MacOs,
    Windows,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenatSafeFlags {
    pub platform: Platform,
    pub primary: &'static str,
    pub fallback: Option<&'static str>,
}

#[must_use]
pub fn flags_for(platform: Platform) -> OpenatSafeFlags {
    match platform {
        Platform::Linux => OpenatSafeFlags {
            platform,
            primary: "openat2(RESOLVE_BENEATH | RESOLVE_NO_SYMLINKS)",
            fallback: Some("openat(O_NOFOLLOW + parent_fd + basename)"),
        },
        Platform::MacOs => OpenatSafeFlags {
            platform,
            primary: "open(O_NOFOLLOW_ANY)",
            fallback: Some("component check + openat(O_NOFOLLOW + parent_fd + basename)"),
        },
        Platform::Windows => OpenatSafeFlags {
            platform,
            primary: "CreateFileW(FILE_FLAG_OPEN_REPARSE_POINT)",
            fallback: Some("AppContainer + Job Objects (D-157)"),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linux_uses_resolve_beneath_and_no_symlinks() {
        let f = flags_for(Platform::Linux);
        assert!(f.primary.contains("RESOLVE_BENEATH"));
        assert!(f.primary.contains("RESOLVE_NO_SYMLINKS"));
    }

    #[test]
    fn macos_uses_o_nofollow_any() {
        let f = flags_for(Platform::MacOs);
        assert!(f.primary.contains("O_NOFOLLOW_ANY"));
    }

    #[test]
    fn windows_uses_open_reparse_point() {
        let f = flags_for(Platform::Windows);
        assert!(f.primary.contains("FILE_FLAG_OPEN_REPARSE_POINT"));
    }
}
