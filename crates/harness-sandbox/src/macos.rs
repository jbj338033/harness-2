// IMPLEMENTS: D-053
use crate::{Sandbox, SandboxError, ScopePolicy, sbpl};
use std::process::Command;

const SANDBOX_BIN: &str = "/usr/bin/sandbox-exec";

pub struct SandboxExec;

impl Sandbox for SandboxExec {
    fn name(&self) -> &'static str {
        "sandbox-exec"
    }

    fn wrap(&self, command: Command, policy: &ScopePolicy) -> Result<Command, SandboxError> {
        let profile = sbpl::render_sbpl(policy);
        let mut wrapped = Command::new(SANDBOX_BIN);
        wrapped.arg("-p").arg(profile);
        wrapped.arg("--");
        wrapped.arg(command.get_program());
        for arg in command.get_args() {
            wrapped.arg(arg);
        }
        if let Some(cwd) = command.get_current_dir() {
            wrapped.current_dir(cwd);
        }
        for (k, v) in command.get_envs() {
            match v {
                Some(val) => {
                    wrapped.env(k, val);
                }
                None => {
                    wrapped.env_remove(k);
                }
            }
        }
        Ok(wrapped)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_command_with_sandboxer() {
        let mut original = Command::new("/usr/bin/true");
        original.arg("--flag").arg("value");
        let wrapped = SandboxExec
            .wrap(original, &ScopePolicy::read_only())
            .unwrap();
        assert_eq!(wrapped.get_program(), SANDBOX_BIN);
        let args: Vec<&std::ffi::OsStr> = wrapped.get_args().collect();
        assert_eq!(args[0], "-p");
        let profile = args[1].to_string_lossy();
        assert!(profile.contains("(deny default)"));
        assert_eq!(args[2], "--");
        assert_eq!(args[3], "/usr/bin/true");
        assert_eq!(args[4], "--flag");
        assert_eq!(args[5], "value");
    }

    #[test]
    fn preserves_cwd_and_env() {
        let mut original = Command::new("/usr/bin/true");
        original.current_dir("/tmp");
        original.env("FOO", "bar");
        let wrapped = SandboxExec
            .wrap(original, &ScopePolicy::read_only())
            .unwrap();
        assert_eq!(
            wrapped.get_current_dir().map(|p| p.to_str().unwrap()),
            Some("/tmp")
        );
        let envs: Vec<_> = wrapped.get_envs().collect();
        assert!(
            envs.iter()
                .any(|(k, v)| *k == "FOO" && v.is_some_and(|x| x == "bar"))
        );
    }
}
