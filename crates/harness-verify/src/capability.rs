// IMPLEMENTS: D-171
//! Per D-171e, "destructive" is defined per capability axis (FS / NET / PROC)
//! plus an explicit tool allowlist. The verify loop uses these to decide
//! whether an action needs an extra approval round before running.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    Fs,
    Net,
    Proc,
}

/// Tools whose mere invocation is treated as destructive — they side-effect
/// outside the harness sandbox boundary even when arguments look harmless.
pub const DESTRUCTIVE_TOOL_ALLOWLIST: &[&str] = &[
    "fs.write",
    "fs.edit",
    "fs.delete",
    "shell.bash",
    "git.push",
    "browser.navigate",
];

#[must_use]
pub fn is_destructive(tool_name: &str, capabilities: &[Capability]) -> bool {
    if DESTRUCTIVE_TOOL_ALLOWLIST.contains(&tool_name) {
        return true;
    }
    // Net by itself is not destructive — read-only HTTP fetches don't
    // mutate the host. Fs and Proc do, so either is enough to escalate.
    capabilities.contains(&Capability::Fs) || capabilities.contains(&Capability::Proc)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allowlisted_tool_is_destructive() {
        assert!(is_destructive("fs.write", &[]));
        assert!(is_destructive("git.push", &[]));
    }

    #[test]
    fn fs_capability_is_destructive() {
        assert!(is_destructive("custom.tool", &[Capability::Fs]));
    }

    #[test]
    fn proc_capability_is_destructive() {
        assert!(is_destructive("custom.tool", &[Capability::Proc]));
    }

    #[test]
    fn pure_net_alone_is_not_destructive_unless_in_allowlist() {
        assert!(!is_destructive("custom.fetch", &[Capability::Net]));
    }

    #[test]
    fn empty_caps_and_unknown_tool_is_safe() {
        assert!(!is_destructive("custom.fetch", &[]));
    }
}
