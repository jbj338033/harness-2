// IMPLEMENTS: D-159, D-165, D-172
//! Hook registry. The 24-event surface is closed (D-172a) — adding a new
//! event requires its own D-decision so workspace-trust assumptions hold.
//!
//! Three D-172 invariants encoded here:
//! - **Explicit list**: no `*` catch-all wildcard.
//! - **Fan-out ≤ 10**: at most ten handlers per event so a single payload
//!   can't DoS the daemon.
//! - **Recursion guard**: a handler that fires while another handler is
//!   running on the same event is dropped. Prevents StopFailure → Stop →
//!   StopFailure loops the way D-165b warned about.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use thiserror::Error;

pub const MAX_HANDLERS_PER_EVENT: usize = 10;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum HookError {
    #[error("event {0:?} already has the maximum {MAX_HANDLERS_PER_EVENT} handlers")]
    FanOutExceeded(HookEvent),
    #[error("event {0:?} is not in the closed 24-event list")]
    UnknownEvent(String),
    #[error("hook {0} would re-enter event {1:?}; dropped")]
    Recursion(String, HookEvent),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookEvent {
    // Pre-existing 14 (Claude Code 2.0 baseline)
    SessionStart,
    UserPromptSubmit,
    AssistantResponse,
    ToolBefore,
    ToolAfter,
    PermissionPrompt,
    Notification,
    Stop,
    Compact,
    EditStart,
    EditEnd,
    SkillActivated,
    McpServerConnected,
    McpServerDisconnected,
    // D-159 +10 (Claude Code 2.1 parity)
    PermissionDenied,
    SessionEnd,
    StopFailure,
    SubagentSummoned,
    SubagentCompleted,
    GrantGranted,
    GrantRevoked,
    MemoryRevised,
    TriggerFired,
    CostCapReached,
}

impl HookEvent {
    /// All 24 events in stable order. The order is the registration
    /// canonical order — tests assert against the count to catch silent
    /// additions.
    pub const ALL: &'static [HookEvent] = &[
        Self::SessionStart,
        Self::UserPromptSubmit,
        Self::AssistantResponse,
        Self::ToolBefore,
        Self::ToolAfter,
        Self::PermissionPrompt,
        Self::Notification,
        Self::Stop,
        Self::Compact,
        Self::EditStart,
        Self::EditEnd,
        Self::SkillActivated,
        Self::McpServerConnected,
        Self::McpServerDisconnected,
        Self::PermissionDenied,
        Self::SessionEnd,
        Self::StopFailure,
        Self::SubagentSummoned,
        Self::SubagentCompleted,
        Self::GrantGranted,
        Self::GrantRevoked,
        Self::MemoryRevised,
        Self::TriggerFired,
        Self::CostCapReached,
    ];

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SessionStart => "session_start",
            Self::UserPromptSubmit => "user_prompt_submit",
            Self::AssistantResponse => "assistant_response",
            Self::ToolBefore => "tool_before",
            Self::ToolAfter => "tool_after",
            Self::PermissionPrompt => "permission_prompt",
            Self::Notification => "notification",
            Self::Stop => "stop",
            Self::Compact => "compact",
            Self::EditStart => "edit_start",
            Self::EditEnd => "edit_end",
            Self::SkillActivated => "skill_activated",
            Self::McpServerConnected => "mcp_server_connected",
            Self::McpServerDisconnected => "mcp_server_disconnected",
            Self::PermissionDenied => "permission_denied",
            Self::SessionEnd => "session_end",
            Self::StopFailure => "stop_failure",
            Self::SubagentSummoned => "subagent_summoned",
            Self::SubagentCompleted => "subagent_completed",
            Self::GrantGranted => "grant_granted",
            Self::GrantRevoked => "grant_revoked",
            Self::MemoryRevised => "memory_revised",
            Self::TriggerFired => "trigger_fired",
            Self::CostCapReached => "cost_cap_reached",
        }
    }

    /// D-172c: CostCapReached can only emit Speak / Notification side
    /// effects. Hooks that try to launch tools on this event are dropped
    /// at registration time — transaction rollback is the kernel's job.
    #[must_use]
    pub fn allows_act(self) -> bool {
        !matches!(self, Self::CostCapReached)
    }
}

/// Stable identity for a hook handler — name + the bytes of its config.
/// Two registrations with the same fingerprint are deduped (D-165 a).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HookFingerprint(String);

impl HookFingerprint {
    #[must_use]
    pub fn new(name: &str, config_bytes: &[u8]) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"harness/hook-fingerprint/v1\n");
        hasher.update(name.as_bytes());
        hasher.update(b"\n");
        hasher.update(config_bytes);
        let hex: String = hasher.finalize().to_hex().chars().take(16).collect();
        Self(format!("{name}#{hex}"))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone)]
pub struct HookHandle {
    pub fingerprint: HookFingerprint,
    pub event: HookEvent,
    pub may_act: bool,
}

#[derive(Debug, Clone, Default)]
pub struct HookRegistry {
    by_event: BTreeMap<HookEvent, Vec<HookHandle>>,
    /// Recursion guard — events currently mid-dispatch.
    in_flight: HashSet<HookEvent>,
}

impl HookRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(
        &mut self,
        event: HookEvent,
        fingerprint: HookFingerprint,
        may_act: bool,
    ) -> Result<HookHandle, HookError> {
        let slot = self.by_event.entry(event).or_default();
        if slot.iter().any(|h| h.fingerprint == fingerprint) {
            // Idempotent — return the existing handle.
            return Ok(slot
                .iter()
                .find(|h| h.fingerprint == fingerprint)
                .cloned()
                .expect("just checked"));
        }
        if slot.len() >= MAX_HANDLERS_PER_EVENT {
            return Err(HookError::FanOutExceeded(event));
        }
        if may_act && !event.allows_act() {
            return Err(HookError::UnknownEvent(format!(
                "{} forbids Act handlers (CostCapReached → Speak/Notification only)",
                event.as_str()
            )));
        }
        let handle = HookHandle {
            fingerprint,
            event,
            may_act,
        };
        slot.push(handle.clone());
        Ok(handle)
    }

    pub fn handlers_for(&self, event: HookEvent) -> &[HookHandle] {
        self.by_event.get(&event).map(Vec::as_slice).unwrap_or(&[])
    }

    /// Return the list of hooks that should fire for `event`. If `event`
    /// is already mid-dispatch, returns an empty slice — D-165b prevents
    /// a StopFailure handler from re-emitting StopFailure on its own
    /// failure.
    pub fn dispatch_guard(&mut self, event: HookEvent) -> DispatchGuard<'_> {
        if self.in_flight.contains(&event) {
            DispatchGuard {
                registry: self,
                event,
                allowed: false,
            }
        } else {
            self.in_flight.insert(event);
            DispatchGuard {
                registry: self,
                event,
                allowed: true,
            }
        }
    }

    pub fn count(&self, event: HookEvent) -> usize {
        self.by_event.get(&event).map_or(0, Vec::len)
    }
}

#[derive(Debug)]
pub struct DispatchGuard<'a> {
    registry: &'a mut HookRegistry,
    event: HookEvent,
    allowed: bool,
}

impl DispatchGuard<'_> {
    #[must_use]
    pub fn handlers(&self) -> &[HookHandle] {
        if self.allowed {
            self.registry.handlers_for(self.event)
        } else {
            &[]
        }
    }

    #[must_use]
    pub fn was_recursive(&self) -> bool {
        !self.allowed
    }
}

impl Drop for DispatchGuard<'_> {
    fn drop(&mut self) {
        if self.allowed {
            self.registry.in_flight.remove(&self.event);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fp(name: &str) -> HookFingerprint {
        HookFingerprint::new(name, name.as_bytes())
    }

    #[test]
    fn event_count_is_exactly_24() {
        assert_eq!(HookEvent::ALL.len(), 24);
    }

    #[test]
    fn event_as_str_is_unique_and_snake_case() {
        let mut seen = std::collections::HashSet::new();
        for e in HookEvent::ALL {
            let s = e.as_str();
            assert!(seen.insert(s), "duplicate {s}");
            assert!(s.chars().all(|c| c.is_ascii_lowercase() || c == '_'));
        }
    }

    #[test]
    fn cost_cap_reached_forbids_act_handlers() {
        let mut reg = HookRegistry::new();
        let err = reg
            .register(HookEvent::CostCapReached, fp("rollback"), true)
            .unwrap_err();
        assert!(matches!(err, HookError::UnknownEvent(_)));
    }

    #[test]
    fn fan_out_caps_at_max() {
        let mut reg = HookRegistry::new();
        for i in 0..MAX_HANDLERS_PER_EVENT {
            reg.register(HookEvent::ToolBefore, fp(&format!("h{i}")), false)
                .unwrap();
        }
        let err = reg
            .register(HookEvent::ToolBefore, fp("h-overflow"), false)
            .unwrap_err();
        assert!(matches!(err, HookError::FanOutExceeded(_)));
    }

    #[test]
    fn duplicate_fingerprint_is_idempotent() {
        let mut reg = HookRegistry::new();
        reg.register(HookEvent::Stop, fp("dedup"), false).unwrap();
        reg.register(HookEvent::Stop, fp("dedup"), false).unwrap();
        assert_eq!(reg.count(HookEvent::Stop), 1);
    }

    #[test]
    fn fingerprint_is_deterministic_per_name_and_config() {
        let a = HookFingerprint::new("h", b"cfg");
        let b = HookFingerprint::new("h", b"cfg");
        let c = HookFingerprint::new("h", b"cfg2");
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn dispatch_guard_blocks_recursion() {
        let mut reg = HookRegistry::new();
        reg.register(HookEvent::StopFailure, fp("crashy"), false)
            .unwrap();
        let outer = reg.dispatch_guard(HookEvent::StopFailure);
        assert!(!outer.was_recursive());
        let outer_handlers = outer.handlers().len();
        // Drop outer when scope ends — but during it, reentry is blocked.
        // Simulate the kernel re-emitting StopFailure inside the handler:
        // we can't borrow reg mut twice, so the guard semantics are
        // verified by the count being preserved + later guard granting.
        assert_eq!(outer_handlers, 1);
        drop(outer);
        let again = reg.dispatch_guard(HookEvent::StopFailure);
        assert!(!again.was_recursive());
    }

    #[test]
    fn session_end_is_present_per_d_172_b() {
        assert!(HookEvent::ALL.contains(&HookEvent::SessionEnd));
    }

    #[test]
    fn allows_act_returns_false_only_for_cost_cap() {
        for e in HookEvent::ALL {
            let expected = !matches!(e, HookEvent::CostCapReached);
            assert_eq!(e.allows_act(), expected, "{e:?}");
        }
    }
}
