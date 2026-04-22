// IMPLEMENTS: D-157
//! Windows AppContainer + Job Objects sandbox descriptor. Selected
//! by `harness doctor sandbox` when the platform is Windows. We hold
//! a pure-data spec here; the actual NT API binding lives in the
//! sandbox crate behind a `#[cfg(windows)]` module.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobObjectLimits {
    pub active_process_limit: u32,
    pub job_memory_bytes: u64,
    pub kill_on_job_close: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppContainerProfile {
    pub package_name: String,
    pub capabilities: Vec<String>,
    pub job_object: JobObjectLimits,
}

#[must_use]
pub fn default_profile() -> AppContainerProfile {
    AppContainerProfile {
        package_name: "com.harness.daemon".into(),
        capabilities: Vec::new(),
        job_object: JobObjectLimits {
            active_process_limit: 64,
            job_memory_bytes: 4 * 1024 * 1024 * 1024,
            kill_on_job_close: true,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_profile_is_capability_empty() {
        let p = default_profile();
        assert!(p.capabilities.is_empty());
    }

    #[test]
    fn job_object_kills_children_on_close_by_default() {
        assert!(default_profile().job_object.kill_on_job_close);
    }

    #[test]
    fn job_object_memory_cap_is_4gib() {
        assert_eq!(
            default_profile().job_object.job_memory_bytes,
            4 * 1024 * 1024 * 1024
        );
    }
}
