// IMPLEMENTS: D-053
use crate::{ScopePolicy, Tier};
use std::fmt::Write as _;

/// Render the SBPL profile macOS' sandboxer consumes via `-p`.
///
/// SBPL is `(allow|deny) <syscall-class> ...` over a default-deny base. Each
/// axis maps to a small set of classes; the writable-paths list expands to
/// `(literal …)` allow rules under `file-write*`.
#[must_use]
pub fn render_sbpl(policy: &ScopePolicy) -> String {
    let mut out = String::with_capacity(512);
    out.push_str("(version 1)\n(deny default)\n");

    out.push_str("(allow process-exec (literal \"/usr/lib/dyld\"))\n");
    out.push_str("(allow file-read* (literal \"/dev/null\") (literal \"/dev/random\") (literal \"/dev/urandom\"))\n");

    match policy.fs {
        Tier::None => {}
        Tier::Read | Tier::Write | Tier::Exec | Tier::Full => {
            out.push_str("(allow file-read*)\n");
        }
    }
    if matches!(policy.fs, Tier::Write | Tier::Exec | Tier::Full) {
        if policy.fs_writable_paths.is_empty() {
            out.push_str("(allow file-write*)\n");
        } else {
            for p in &policy.fs_writable_paths {
                let s = p.display().to_string();
                let escaped = s.replace('\\', "\\\\").replace('"', "\\\"");
                writeln!(out, "(allow file-write* (subpath \"{escaped}\"))").unwrap();
            }
        }
    }

    if !matches!(policy.net, Tier::None) {
        out.push_str("(allow network-outbound)\n");
        out.push_str("(allow network-bind)\n");
    }

    match policy.proc_ {
        Tier::None => {}
        Tier::Read => {
            out.push_str("(allow process-info-pidinfo)\n");
        }
        _ => {
            out.push_str("(allow process-fork)\n");
            out.push_str("(allow process-exec)\n");
            out.push_str("(allow signal)\n");
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deny_default_always_emitted() {
        let s = render_sbpl(&ScopePolicy::read_only());
        assert!(s.contains("(deny default)"));
    }

    #[test]
    fn read_only_has_no_write_allow() {
        let s = render_sbpl(&ScopePolicy::read_only());
        assert!(s.contains("(allow file-read*)"));
        assert!(!s.contains("(allow file-write*)"));
    }

    #[test]
    fn editor_with_writable_paths_emits_subpath_rules() {
        let s = render_sbpl(&ScopePolicy::editor(vec!["/tmp/work".into()]));
        assert!(s.contains("(allow file-write* (subpath \"/tmp/work\"))"));
        assert!(!s.contains("(allow file-write*)\n"));
    }

    #[test]
    fn editor_with_no_paths_falls_back_to_unrestricted_write() {
        let s = render_sbpl(&ScopePolicy::editor(Vec::new()));
        assert!(s.contains("(allow file-write*)\n"));
    }

    #[test]
    fn full_policy_emits_network_and_exec_class() {
        let s = render_sbpl(&ScopePolicy::full());
        assert!(s.contains("(allow network-outbound)"));
        assert!(s.contains("(allow process-fork)"));
    }

    #[test]
    fn quotes_in_paths_are_escaped() {
        let s = render_sbpl(&ScopePolicy::editor(vec!["/tmp/has\"quote".into()]));
        assert!(s.contains("\\\""), "got: {s}");
    }
}
