// IMPLEMENTS: D-012, D-053
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::Command;
use thiserror::Error;

#[cfg(not(target_os = "macos"))]
mod fallback;
#[cfg(target_os = "macos")]
mod macos;
mod sbpl;

pub use sbpl::render_sbpl;

#[derive(Debug, Error)]
pub enum SandboxError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("backend not available on this platform: {0}")]
    Unsupported(String),
}

/// One axis of access. The combination FS×NET×PROC fully describes what an
/// agent invocation may touch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tier {
    None,
    Read,
    Write,
    Exec,
    Full,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScopePolicy {
    pub fs: Tier,
    pub net: Tier,
    pub proc_: Tier,
    #[serde(default)]
    pub fs_writable_paths: Vec<PathBuf>,
}

impl ScopePolicy {
    /// Minimum-trust policy — no network, no spawn, read-only filesystem.
    #[must_use]
    pub fn read_only() -> Self {
        Self {
            fs: Tier::Read,
            net: Tier::None,
            proc_: Tier::None,
            fs_writable_paths: Vec::new(),
        }
    }

    /// What an editor agent typically needs.
    #[must_use]
    pub fn editor(writable: Vec<PathBuf>) -> Self {
        Self {
            fs: Tier::Write,
            net: Tier::None,
            proc_: Tier::Exec,
            fs_writable_paths: writable,
        }
    }

    /// Wide-open — used by the daemon supervisor itself.
    #[must_use]
    pub fn full() -> Self {
        Self {
            fs: Tier::Full,
            net: Tier::Full,
            proc_: Tier::Full,
            fs_writable_paths: Vec::new(),
        }
    }
}

/// The platform-agnostic surface every sandbox backend implements. The
/// signature returns a `Command` so callers can attach env vars / cwd
/// without the trait having to know about them.
pub trait Sandbox {
    fn name(&self) -> &'static str;

    /// Wrap the command so that, when spawned, it runs under the sandbox
    /// dictated by `policy`. On platforms with no enforcement available the
    /// fallback backend returns the command unchanged but reports its name
    /// as `"noop"` so callers can warn the user.
    ///
    /// # Errors
    /// Returns [`SandboxError::Unsupported`] if the host kernel does not
    /// expose the primitives the backend needs (eg. landlock disabled).
    fn wrap(&self, command: Command, policy: &ScopePolicy) -> Result<Command, SandboxError>;
}

#[must_use]
pub fn default() -> Box<dyn Sandbox + Send + Sync> {
    #[cfg(target_os = "macos")]
    {
        Box::new(macos::SandboxExec)
    }
    #[cfg(not(target_os = "macos"))]
    {
        Box::new(fallback::Noop)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_some_named_backend() {
        let s = default();
        assert!(!s.name().is_empty());
    }

    #[test]
    fn wrap_preserves_program() {
        let s = default();
        let original = Command::new("/usr/bin/true");
        let wrapped = s.wrap(original, &ScopePolicy::read_only()).unwrap();
        // Either the wrapper is /usr/bin/sandbox-exec on macOS, or the same
        // /usr/bin/true noop everywhere else — both must be a runnable path.
        let prog = wrapped.get_program();
        assert!(!prog.is_empty());
    }
}
