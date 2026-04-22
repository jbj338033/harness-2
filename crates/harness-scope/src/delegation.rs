// IMPLEMENTS: D-187
//! Subagent delegation scope. When the main agent summons a worker the
//! summon body must carry both the human principal that authorised the
//! original session and a strict subset of that principal's scope. MIT
//! arXiv:2501.09674 calls this the "delegation chain" — every hop must
//! be ≤ the prior hop's authority so a worker can never re-escalate.
//!
//! Three guarantees:
//! - `DelegationScope::is_subset_of(parent)` is a Boolean test the
//!   summoner must pass before the worker boots.
//! - Authority bits never get added by tightening — once a category is
//!   off it stays off down-chain.
//! - The principal_id rides with every event the worker emits so the
//!   audit trail collapses back to one human.

use crate::ScopePolicy;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PrincipalId(pub String);

impl PrincipalId {
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DelegationScope {
    pub policy: ScopePolicy,
    /// Caller can also forbid the worker from spawning further workers
    /// (chain depth = 1). Default is true to keep the typical case
    /// reasonable.
    #[serde(default = "default_true")]
    pub may_summon: bool,
    /// Hard cap on tool calls the worker can make before its summon body
    /// expires. None = inherit from session-level limits.
    #[serde(default)]
    pub max_tool_calls: Option<u32>,
}

fn default_true() -> bool {
    true
}

impl DelegationScope {
    /// True iff every category in `self` is contained in `parent`.
    #[must_use]
    pub fn is_subset_of(&self, parent: &Self) -> bool {
        if !is_dns_subset(&self.policy.allowed_dns, &parent.policy.allowed_dns) {
            return false;
        }
        if !is_string_subset(
            &self.policy.allowed_http_prefixes,
            &parent.policy.allowed_http_prefixes,
        ) {
            return false;
        }
        if !is_string_subset(
            &self.policy.allowed_shell_programs,
            &parent.policy.allowed_shell_programs,
        ) {
            return false;
        }
        if self.may_summon && !parent.may_summon {
            return false;
        }
        match (self.max_tool_calls, parent.max_tool_calls) {
            (Some(c), Some(p)) if c > p => return false,
            (Some(_), None) => {} // child tightening is fine
            _ => {}
        }
        true
    }
}

fn is_string_subset(child: &[String], parent: &[String]) -> bool {
    child.iter().all(|c| parent.iter().any(|p| p == c))
}

fn is_dns_subset(child: &[String], parent: &[String]) -> bool {
    child.iter().all(|c| {
        parent.iter().any(|p| {
            if p == c {
                return true;
            }
            // Wildcard parent accepts narrower child.
            if let Some(suffix) = p.strip_prefix("*.") {
                if c == suffix {
                    return true;
                }
                if c.ends_with(&format!(".{suffix}")) {
                    return true;
                }
            }
            false
        })
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SummonBody {
    pub principal_id: PrincipalId,
    pub parent_session_id: String,
    pub delegation_scope: DelegationScope,
    /// Free-form task brief the worker receives.
    pub task: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DelegationError {
    /// Child requested a scope wider than the parent — escalation refused.
    ScopeWiderThanParent,
    /// Child wants to summon further workers but parent forbids it.
    SummonDepthExceeded,
}

impl std::fmt::Display for DelegationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ScopeWiderThanParent => write!(f, "delegation scope is not a subset of parent"),
            Self::SummonDepthExceeded => {
                write!(f, "child requested may_summon but parent forbids it")
            }
        }
    }
}

impl std::error::Error for DelegationError {}

/// Validate a summon body before booting the worker. Returns Ok only when
/// the child's delegation_scope is ≤ parent's per `is_subset_of`.
pub fn validate_summon(parent: &DelegationScope, body: &SummonBody) -> Result<(), DelegationError> {
    if !body.delegation_scope.is_subset_of(parent) {
        if body.delegation_scope.may_summon && !parent.may_summon {
            return Err(DelegationError::SummonDepthExceeded);
        }
        return Err(DelegationError::ScopeWiderThanParent);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parent_scope() -> DelegationScope {
        DelegationScope {
            policy: ScopePolicy {
                allowed_dns: vec!["api.example.com".into(), "*.docs.example.com".into()],
                allowed_http_prefixes: vec!["https://api.example.com/v1/".into()],
                allowed_shell_programs: vec!["cargo".into(), "git".into()],
            },
            may_summon: true,
            max_tool_calls: Some(20),
        }
    }

    #[test]
    fn equal_scope_is_a_subset() {
        let p = parent_scope();
        assert!(p.is_subset_of(&p));
    }

    #[test]
    fn child_with_strict_subset_dns_is_subset() {
        let mut child = parent_scope();
        child.policy.allowed_dns = vec!["api.example.com".into()];
        assert!(child.is_subset_of(&parent_scope()));
    }

    #[test]
    fn child_dns_inside_wildcard_is_subset() {
        let mut child = parent_scope();
        child.policy.allowed_dns = vec!["v2.docs.example.com".into()];
        assert!(child.is_subset_of(&parent_scope()));
    }

    #[test]
    fn child_with_extra_dns_is_not_subset() {
        let mut child = parent_scope();
        child.policy.allowed_dns.push("evil.com".into());
        assert!(!child.is_subset_of(&parent_scope()));
    }

    #[test]
    fn child_summon_when_parent_forbids_is_not_subset() {
        let parent = DelegationScope {
            may_summon: false,
            ..parent_scope()
        };
        let child = parent_scope(); // may_summon = true
        assert!(!child.is_subset_of(&parent));
    }

    #[test]
    fn child_with_higher_tool_call_cap_is_not_subset() {
        let parent = parent_scope();
        let child = DelegationScope {
            max_tool_calls: Some(100),
            ..parent.clone()
        };
        assert!(!child.is_subset_of(&parent));
    }

    #[test]
    fn child_with_lower_tool_call_cap_is_subset() {
        let parent = parent_scope();
        let child = DelegationScope {
            max_tool_calls: Some(5),
            ..parent.clone()
        };
        assert!(child.is_subset_of(&parent));
    }

    #[test]
    fn validate_summon_returns_ok_for_subset() {
        let body = SummonBody {
            principal_id: PrincipalId::new("user-1"),
            parent_session_id: "s1".into(),
            delegation_scope: parent_scope(),
            task: "ship the change".into(),
        };
        assert!(validate_summon(&parent_scope(), &body).is_ok());
    }

    #[test]
    fn validate_summon_blocks_wider_scope() {
        let parent = parent_scope();
        let mut wide = parent.clone();
        wide.policy.allowed_shell_programs.push("rm".into());
        let body = SummonBody {
            principal_id: PrincipalId::new("user-1"),
            parent_session_id: "s1".into(),
            delegation_scope: wide,
            task: "x".into(),
        };
        assert_eq!(
            validate_summon(&parent, &body),
            Err(DelegationError::ScopeWiderThanParent)
        );
    }

    #[test]
    fn validate_summon_distinguishes_summon_depth() {
        let parent = DelegationScope {
            may_summon: false,
            ..parent_scope()
        };
        let body = SummonBody {
            principal_id: PrincipalId::new("user-1"),
            parent_session_id: "s1".into(),
            delegation_scope: DelegationScope {
                may_summon: true,
                ..parent.clone()
            },
            task: "x".into(),
        };
        assert_eq!(
            validate_summon(&parent, &body),
            Err(DelegationError::SummonDepthExceeded)
        );
    }
}
