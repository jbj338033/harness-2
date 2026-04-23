//! Experimental remote-runtime descriptors. The default backend is
//! Firecracker microVM — a session can ship its sandbox to a remote
//! VM with the same `harness-scope` policy and stream events back.
//!
//! This crate is pure data: lifecycle states, the kernel + rootfs
//! pin, and the resource budget. Wiring against the Firecracker
//! HTTP API lives in a future `harness-tools-firecracker` crate.

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RemoteRuntimeBackend {
    Firecracker,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RemoteVmState {
    NotStarted,
    Booting,
    Running,
    Paused,
    Halted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteKernelPin {
    pub kernel_image_sha256_hex: String,
    pub rootfs_sha256_hex: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteResourceBudget {
    pub vcpus: u8,
    pub memory_mib: u32,
    pub disk_gib: u32,
    pub idle_timeout_minutes: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteSession {
    pub session_id: String,
    pub backend: RemoteRuntimeBackend,
    pub state: RemoteVmState,
    pub kernel_pin: RemoteKernelPin,
    pub budget: RemoteResourceBudget,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum RemoteRuntimeError {
    #[error("budget invalid: {0}")]
    InvalidBudget(&'static str),
    #[error("kernel pin missing sha256")]
    MissingKernelPin,
    #[error("transition refused: {from:?} → {to:?}")]
    BadTransition {
        from: RemoteVmState,
        to: RemoteVmState,
    },
}

impl RemoteResourceBudget {
    pub fn validate(&self) -> Result<(), RemoteRuntimeError> {
        if self.vcpus == 0 {
            return Err(RemoteRuntimeError::InvalidBudget("vcpus must be > 0"));
        }
        if self.memory_mib < 256 {
            return Err(RemoteRuntimeError::InvalidBudget(
                "memory_mib must be >= 256",
            ));
        }
        if self.disk_gib == 0 {
            return Err(RemoteRuntimeError::InvalidBudget("disk_gib must be > 0"));
        }
        if self.idle_timeout_minutes == 0 {
            return Err(RemoteRuntimeError::InvalidBudget(
                "idle_timeout_minutes must be > 0",
            ));
        }
        Ok(())
    }
}

pub fn validate_kernel_pin(pin: &RemoteKernelPin) -> Result<(), RemoteRuntimeError> {
    if pin.kernel_image_sha256_hex.trim().is_empty() || pin.rootfs_sha256_hex.trim().is_empty() {
        return Err(RemoteRuntimeError::MissingKernelPin);
    }
    Ok(())
}

pub fn transition(
    from: RemoteVmState,
    to: RemoteVmState,
) -> Result<RemoteVmState, RemoteRuntimeError> {
    use RemoteVmState::*;
    let ok = matches!(
        (from, to),
        (NotStarted, Booting)
            | (Booting, Running)
            | (Running, Paused)
            | (Paused, Running)
            | (Running, Halted)
            | (Paused, Halted)
            | (Booting, Halted)
    );
    if ok {
        Ok(to)
    } else {
        Err(RemoteRuntimeError::BadTransition { from, to })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn budget() -> RemoteResourceBudget {
        RemoteResourceBudget {
            vcpus: 2,
            memory_mib: 1024,
            disk_gib: 8,
            idle_timeout_minutes: 30,
        }
    }

    fn pin() -> RemoteKernelPin {
        RemoteKernelPin {
            kernel_image_sha256_hex: "abc".into(),
            rootfs_sha256_hex: "def".into(),
        }
    }

    #[test]
    fn full_budget_validates() {
        assert!(budget().validate().is_ok());
    }

    #[test]
    fn zero_vcpus_refused() {
        let mut b = budget();
        b.vcpus = 0;
        assert!(b.validate().is_err());
    }

    #[test]
    fn missing_kernel_pin_refused() {
        let mut p = pin();
        p.kernel_image_sha256_hex.clear();
        assert!(matches!(
            validate_kernel_pin(&p),
            Err(RemoteRuntimeError::MissingKernelPin)
        ));
    }

    #[test]
    fn boot_then_run_then_halt_passes() {
        let s = transition(RemoteVmState::NotStarted, RemoteVmState::Booting).unwrap();
        let s = transition(s, RemoteVmState::Running).unwrap();
        let s = transition(s, RemoteVmState::Halted).unwrap();
        assert_eq!(s, RemoteVmState::Halted);
    }

    #[test]
    fn cannot_resume_halted_vm() {
        let r = transition(RemoteVmState::Halted, RemoteVmState::Running);
        assert!(matches!(r, Err(RemoteRuntimeError::BadTransition { .. })));
    }

    #[test]
    fn pause_resume_loop_passes() {
        let s = transition(RemoteVmState::Running, RemoteVmState::Paused).unwrap();
        let s = transition(s, RemoteVmState::Running).unwrap();
        assert_eq!(s, RemoteVmState::Running);
    }
}
