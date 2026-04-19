use regex_lite::Regex;
use std::path::Path;
use std::sync::OnceLock;

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
        let trimmed = cmd.trim();
        if trimmed.is_empty() {
            return SandboxPolicy::Allow;
        }

        for (pattern, reason) in deny_patterns() {
            if pattern.is_match(trimmed) {
                return SandboxPolicy::Deny {
                    reason: (*reason).to_string(),
                };
            }
        }
        for (pattern, reason) in confirm_patterns() {
            if pattern.is_match(trimmed) {
                return SandboxPolicy::Confirm {
                    reason: (*reason).to_string(),
                };
            }
        }
        SandboxPolicy::Allow
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

fn deny_patterns() -> &'static [(Regex, &'static str)] {
    static CACHE: OnceLock<Vec<(Regex, &'static str)>> = OnceLock::new();
    CACHE.get_or_init(|| {
        vec![
            (
                Regex::new(
                    r"(?:^|[\s;&|])rm\s+(-[rRfi]+\s+)*(-[rRfi]+\s+)?/\s*($|[^a-zA-Z0-9_./-])",
                )
                .unwrap(),
                "destructive wipe of root filesystem",
            ),
            (
                Regex::new(r"(?m)\bmkfs(\.\w+)?\b").unwrap(),
                "filesystem creation",
            ),
            (
                Regex::new(r"\bdd\b.*\bof=/dev/(sd|nvme|hd|disk)\w*").unwrap(),
                "overwrite raw disk device",
            ),
            (
                Regex::new(r":\s*\(\s*\)\s*\{.*:\|:\s*&\s*\}\s*;\s*:").unwrap(),
                "fork bomb",
            ),
            (
                Regex::new(r"\bchmod\b\s+-R\s+0?777\s+/").unwrap(),
                "chmod 777 on filesystem root",
            ),
            (
                Regex::new(r">\s*/dev/(sd|nvme|hd|disk)\w*").unwrap(),
                "redirect to raw disk device",
            ),
        ]
    })
}

fn confirm_patterns() -> &'static [(Regex, &'static str)] {
    static CACHE: OnceLock<Vec<(Regex, &'static str)>> = OnceLock::new();
    CACHE.get_or_init(|| {
        vec![
            (
                Regex::new(r"\bgit\s+push\s+.*--force\b").unwrap(),
                "git force push",
            ),
            (
                Regex::new(r"\bgit\s+reset\s+--hard\b").unwrap(),
                "git reset --hard",
            ),
            (Regex::new(r"^\s*sudo\b").unwrap(), "sudo execution"),
            (
                Regex::new(r"^\s*rm\s+-[rRfi]+\s+[^/]").unwrap(),
                "recursive rm",
            ),
            (Regex::new(r"\bkill\s+-9\b").unwrap(), "force kill"),
        ]
    })
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
