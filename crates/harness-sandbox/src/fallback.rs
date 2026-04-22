// IMPLEMENTS: D-012
use crate::{Sandbox, SandboxError, ScopePolicy};
use std::process::Command;

/// Off-platform stand-in — used until landlock+seccompiler (D-012 Linux),
/// Lima (D-111), and the Wasm backend (D-012 plugins) ship.
pub struct Noop;

impl Sandbox for Noop {
    fn name(&self) -> &'static str {
        "noop"
    }

    fn wrap(&self, command: Command, _policy: &ScopePolicy) -> Result<Command, SandboxError> {
        Ok(command)
    }
}
