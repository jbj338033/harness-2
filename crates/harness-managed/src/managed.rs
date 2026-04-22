// IMPLEMENTS: D-149
//! Managed (detached daemon) and Scheduled (cron + slug-keyed cross-
//! run notes) agent primitives. The "self-authored notes" idea
//! lifted from Devin lives in the `memory_slug` handle — a scheduled
//! run reads + appends to a single named page that survives across
//! invocations.

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ManagedKind {
    Detached,
    Scheduled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManagedAgent {
    pub task_label: String,
    pub kind: ManagedKind,
    pub idle_timeout_minutes: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScheduledAgent {
    pub task_label: String,
    pub cron: String,
    /// Cross-run shared memory slug. Reads & appends always go to
    /// `memory.pages[slug]`.
    pub memory_slug: String,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ManagedError {
    #[error("managed agent missing task_label")]
    MissingTaskLabel,
    #[error("managed agent idle_timeout_minutes must be > 0")]
    NonPositiveIdleTimeout,
    #[error("scheduled agent missing memory_slug — required for cross-run notes")]
    MissingMemorySlug,
    #[error("scheduled agent cron must have 5 whitespace-separated fields, got {0}")]
    BadCron(usize),
}

pub fn validate_managed(agent: &ManagedAgent) -> Result<(), ManagedError> {
    if agent.task_label.trim().is_empty() {
        return Err(ManagedError::MissingTaskLabel);
    }
    if agent.idle_timeout_minutes == 0 {
        return Err(ManagedError::NonPositiveIdleTimeout);
    }
    Ok(())
}

pub fn validate_cron(cron: &str) -> Result<(), ManagedError> {
    let n = cron.split_whitespace().count();
    if n != 5 {
        return Err(ManagedError::BadCron(n));
    }
    Ok(())
}

pub fn validate_scheduled(agent: &ScheduledAgent) -> Result<(), ManagedError> {
    if agent.task_label.trim().is_empty() {
        return Err(ManagedError::MissingTaskLabel);
    }
    if agent.memory_slug.trim().is_empty() {
        return Err(ManagedError::MissingMemorySlug);
    }
    validate_cron(&agent.cron)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn managed() -> ManagedAgent {
        ManagedAgent {
            task_label: "build greenfield".into(),
            kind: ManagedKind::Detached,
            idle_timeout_minutes: 120,
        }
    }

    fn scheduled() -> ScheduledAgent {
        ScheduledAgent {
            task_label: "nightly digest".into(),
            cron: "0 6 * * *".into(),
            memory_slug: "digest-notes".into(),
        }
    }

    #[test]
    fn full_managed_validates() {
        assert!(validate_managed(&managed()).is_ok());
    }

    #[test]
    fn zero_idle_rejected() {
        let mut m = managed();
        m.idle_timeout_minutes = 0;
        assert!(matches!(
            validate_managed(&m),
            Err(ManagedError::NonPositiveIdleTimeout)
        ));
    }

    #[test]
    fn five_field_cron_passes() {
        assert!(validate_cron("*/5 * * * *").is_ok());
    }

    #[test]
    fn three_field_cron_rejected() {
        assert!(matches!(
            validate_cron("0 6 *"),
            Err(ManagedError::BadCron(3))
        ));
    }

    #[test]
    fn scheduled_without_slug_rejected() {
        let mut s = scheduled();
        s.memory_slug.clear();
        assert!(matches!(
            validate_scheduled(&s),
            Err(ManagedError::MissingMemorySlug)
        ));
    }

    #[test]
    fn full_scheduled_validates() {
        assert!(validate_scheduled(&scheduled()).is_ok());
    }
}
