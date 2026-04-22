// IMPLEMENTS: D-065
use crate::bash_ast::{self, BashVerdict};
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SandboxPolicy {
    Allow,
    Confirm { reason: String },
    Deny { reason: String },
}

#[derive(Debug, Clone, Copy)]
pub struct Sandbox;

impl Sandbox {
    #[must_use]
    pub fn evaluate_command(cmd: &str) -> SandboxPolicy {
        match bash_ast::evaluate(cmd) {
            BashVerdict::Allow => SandboxPolicy::Allow,
            BashVerdict::Confirm(reason) => SandboxPolicy::Confirm { reason },
            BashVerdict::Deny(reason) => SandboxPolicy::Deny { reason },
        }
    }

    #[must_use]
    pub fn evaluate_write_path(path: &Path) -> SandboxPolicy {
        const FORBIDDEN_PREFIXES: &[&str] = &[
            "/etc/",
            "/bin/",
            "/sbin/",
            "/usr/bin/",
            "/usr/sbin/",
            "/System/",
            "/dev/",
            "/proc/",
            "/sys/",
            "/boot/",
        ];
        let display = path.display().to_string();
        for p in FORBIDDEN_PREFIXES {
            if display.starts_with(p) {
                return SandboxPolicy::Deny {
                    reason: format!("writing under {p} is forbidden"),
                };
            }
        }
        SandboxPolicy::Allow
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn deny(cmd: &str) {
        match Sandbox::evaluate_command(cmd) {
            SandboxPolicy::Deny { reason } => {
                assert!(!reason.is_empty(), "expected reason for {cmd}");
            }
            other => panic!("expected deny for {cmd:?}, got {other:?}"),
        }
    }
    fn confirm(cmd: &str) {
        match Sandbox::evaluate_command(cmd) {
            SandboxPolicy::Confirm { .. } => {}
            other => panic!("expected confirm for {cmd:?}, got {other:?}"),
        }
    }
    fn allow(cmd: &str) {
        assert_eq!(Sandbox::evaluate_command(cmd), SandboxPolicy::Allow);
    }

    #[test]
    fn denies_catastrophic() {
        deny("rm -rf /");
        deny("rm -rf /  # bye");
        deny("sudo rm -rf /");
        deny("mkfs.ext4 /dev/sda1");
        deny("dd if=/dev/zero of=/dev/sda");
        deny(":(){ :|: & };:");
        deny("chmod -R 777 /");
        deny("echo hi > /dev/sda");
    }

    #[test]
    fn confirms_potentially_intentional() {
        confirm("git push origin main --force");
        confirm("git reset --hard HEAD~5");
        confirm("sudo apt install foo");
        confirm("rm -rf build");
        confirm("kill -9 12345");
    }

    #[test]
    fn allows_normal() {
        allow("cargo test");
        allow("ls -la");
        allow("git status");
        allow("echo hello");
        allow("");
    }

    #[test]
    fn write_path_forbidden_prefixes() {
        match Sandbox::evaluate_write_path(Path::new("/etc/hosts")) {
            SandboxPolicy::Deny { .. } => {}
            other => panic!("expected deny: {other:?}"),
        }
        assert_eq!(
            Sandbox::evaluate_write_path(Path::new("/tmp/foo")),
            SandboxPolicy::Allow
        );
    }
}
