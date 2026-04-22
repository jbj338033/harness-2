// IMPLEMENTS: D-383
//! `RecordPolicy` — government mode is the inverse of legal/privilege
//! mode: provider must NOT retain a copy, and the local store MUST
//! keep an append-only record. The taint engine (D-350) treats this
//! as a complementary tag.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordPolicy {
    /// Provider may log; local may delete (default for everyday work).
    Default,
    /// Privilege-safe (D-356) — local forgets, provider must not log.
    PrivilegeSafe,
    /// Government record (D-383) — provider must not log, local
    /// retains as append-only.
    GovernmentRecord,
}

impl RecordPolicy {
    #[must_use]
    pub fn provider_may_log(self) -> bool {
        matches!(self, RecordPolicy::Default)
    }

    #[must_use]
    pub fn local_is_append_only(self) -> bool {
        matches!(self, RecordPolicy::GovernmentRecord)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetentionRule {
    pub minimum_years: u32,
    pub maximum_years: Option<u32>,
}

impl RetentionRule {
    #[must_use]
    pub fn for_policy(policy: RecordPolicy) -> Self {
        match policy {
            RecordPolicy::Default => Self {
                minimum_years: 0,
                maximum_years: Some(2),
            },
            RecordPolicy::PrivilegeSafe => Self {
                minimum_years: 0,
                maximum_years: Some(0),
            },
            RecordPolicy::GovernmentRecord => Self {
                minimum_years: 7,
                maximum_years: None,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn government_record_provider_must_not_log() {
        assert!(!RecordPolicy::GovernmentRecord.provider_may_log());
        assert!(RecordPolicy::GovernmentRecord.local_is_append_only());
    }

    #[test]
    fn privilege_safe_provider_must_not_log_and_local_forgets() {
        assert!(!RecordPolicy::PrivilegeSafe.provider_may_log());
        assert!(!RecordPolicy::PrivilegeSafe.local_is_append_only());
    }

    #[test]
    fn government_retention_at_least_seven_years() {
        let r = RetentionRule::for_policy(RecordPolicy::GovernmentRecord);
        assert!(r.minimum_years >= 7);
        assert!(r.maximum_years.is_none());
    }

    #[test]
    fn privilege_safe_retention_is_zero() {
        let r = RetentionRule::for_policy(RecordPolicy::PrivilegeSafe);
        assert_eq!(r.minimum_years, 0);
        assert_eq!(r.maximum_years, Some(0));
    }
}
