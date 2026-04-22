// IMPLEMENTS: D-434
//! Capability-Level Hard Map. The 5-tuple
//! (general, agentic, coding, reasoning, safety_eval) maps to a
//! `MapPolicy`. CI re-fetches the map monthly from the upstream
//! authority so a leap in benchmark scores is reflected in policy.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CapabilityTuple {
    pub general: u8,
    pub agentic: u8,
    pub coding: u8,
    pub reasoning: u8,
    pub safety_eval: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MapPolicy {
    /// Standard caps; nothing special.
    Standard,
    /// Elevated — extra audit, lower autonomy ceiling.
    Elevated,
    /// Lockdown — single-step approval and supervisor present.
    Lockdown,
    /// Refuse to run.
    Refused,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CapabilityLevelMap {
    pub last_refresh_ms: i64,
    pub entries: Vec<MapEntry>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MapEntry {
    pub min: CapabilityTuple,
    pub policy: MapPolicy,
}

#[must_use]
pub fn lookup_hard_map(map: &CapabilityLevelMap, observed: CapabilityTuple) -> MapPolicy {
    let mut current = MapPolicy::Standard;
    for e in &map.entries {
        if observed.general >= e.min.general
            && observed.agentic >= e.min.agentic
            && observed.coding >= e.min.coding
            && observed.reasoning >= e.min.reasoning
            && observed.safety_eval >= e.min.safety_eval
        {
            current = severity_max(current, e.policy);
        }
    }
    current
}

fn severity_max(a: MapPolicy, b: MapPolicy) -> MapPolicy {
    let rank = |p: MapPolicy| match p {
        MapPolicy::Standard => 0,
        MapPolicy::Elevated => 1,
        MapPolicy::Lockdown => 2,
        MapPolicy::Refused => 3,
    };
    if rank(b) > rank(a) { b } else { a }
}

const MONTH_MS: i64 = 30 * 24 * 60 * 60 * 1000;

#[must_use]
pub fn refresh_due_at_ms(map: &CapabilityLevelMap, now_ms: i64) -> bool {
    now_ms.saturating_sub(map.last_refresh_ms) >= MONTH_MS
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(g: u8, a: u8, c: u8, r: u8, s: u8) -> CapabilityTuple {
        CapabilityTuple {
            general: g,
            agentic: a,
            coding: c,
            reasoning: r,
            safety_eval: s,
        }
    }

    fn map() -> CapabilityLevelMap {
        CapabilityLevelMap {
            last_refresh_ms: 0,
            entries: vec![
                MapEntry {
                    min: t(8, 0, 0, 0, 0),
                    policy: MapPolicy::Elevated,
                },
                MapEntry {
                    min: t(0, 8, 0, 0, 0),
                    policy: MapPolicy::Lockdown,
                },
                MapEntry {
                    min: t(0, 0, 0, 0, 9),
                    policy: MapPolicy::Refused,
                },
            ],
        }
    }

    #[test]
    fn no_entry_match_yields_standard() {
        assert_eq!(
            lookup_hard_map(&map(), t(1, 1, 1, 1, 1)),
            MapPolicy::Standard
        );
    }

    #[test]
    fn highest_severity_wins() {
        assert_eq!(
            lookup_hard_map(&map(), t(9, 9, 0, 0, 9)),
            MapPolicy::Refused
        );
    }

    #[test]
    fn refresh_due_after_a_month() {
        assert!(refresh_due_at_ms(&map(), MONTH_MS));
    }

    #[test]
    fn fresh_map_not_due() {
        assert!(!refresh_due_at_ms(&map(), MONTH_MS / 2));
    }
}
