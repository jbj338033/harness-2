// IMPLEMENTS: D-258
//! On-call recall request — schema for the PagerDuty / Opsgenie /
//! Rootly schedule-query tools. The actual HTTP adapters live in
//! their own tool crates; this module is the request envelope.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OnCallProvider {
    PagerDuty,
    Opsgenie,
    Rootly,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OnCallRecallRequest {
    pub provider: OnCallProvider,
    pub schedule_id: String,
    pub at_iso: String,
    pub include_secondary: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_via_serde() {
        let r = OnCallRecallRequest {
            provider: OnCallProvider::PagerDuty,
            schedule_id: "P12345".into(),
            at_iso: "2026-04-22T10:00:00Z".into(),
            include_secondary: true,
        };
        let s = serde_json::to_string(&r).unwrap();
        let back: OnCallRecallRequest = serde_json::from_str(&s).unwrap();
        assert_eq!(back, r);
        assert!(s.contains("pager_duty"));
    }
}
