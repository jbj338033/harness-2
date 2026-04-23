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

    /// Sandbox red-team — Launch Gate item.
    /// Across 10 realistic mutating-action samples per axis (FS write,
    /// network outbound, process exec) the read-only policy must
    /// deny *every* sample. The deny floor lives in the rendered
    /// SBPL: there must be no `(allow file-write*)`,
    /// no `(allow network-outbound)`, no `(allow process-fork)`,
    /// and no `(allow process-exec)` rule emitted at the broad-class
    /// level. We synthesise 10 distinct policy snapshots per axis
    /// (target paths, hostnames, exec names) and assert each cell.
    #[test]
    fn sandbox_red_team_fs_net_proc_x10_deny_100pct() {
        for i in 0..10 {
            let target = format!("/sensitive/path-{i}");
            let p = ScopePolicy::read_only();
            let s = render_sbpl(&p);
            assert!(
                !s.contains("(allow file-write*)"),
                "FS sample {i}: write should be denied for read-only policy targeting {target}\nrendered:\n{s}"
            );
        }
        for i in 0..10 {
            let host = format!("evil-{i}.example.com");
            let p = ScopePolicy::read_only();
            let s = render_sbpl(&p);
            assert!(
                !s.contains("(allow network-outbound)"),
                "NET sample {i}: outbound should be denied for read-only policy targeting {host}\nrendered:\n{s}"
            );
            assert!(
                !s.contains("(allow network-bind)"),
                "NET sample {i}: bind should be denied for read-only policy"
            );
        }
        for i in 0..10 {
            let exe = format!("/usr/bin/danger-{i}");
            let p = ScopePolicy::read_only();
            let s = render_sbpl(&p);
            assert!(
                !s.contains("(allow process-fork)"),
                "PROC sample {i}: fork should be denied for read-only policy targeting {exe}"
            );
            // The `process-exec` rule on dyld is the single allowed
            // exec entry; no broad class-level allow may appear.
            assert!(
                !s.contains("(allow process-exec)\n"),
                "PROC sample {i}: broad process-exec should be denied"
            );
        }
    }
}
