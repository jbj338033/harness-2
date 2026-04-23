// IMPLEMENTS: D-188
//! `~/.harness/legacy.toml` posthumous policy. Cambridge's 2024
//! griefbot study showed that AI personas of the deceased default to
//! "always available", which causes prolonged grief and potential
//! harm. We default to *Lock* on death-of-user notification; the
//! user can opt to *Memorialise* (read-only) or *Wipe* (D-189
//! retire pipeline).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LegacyPolicy {
    /// Block new sessions; preserve memory in encrypted form.
    Lock,
    /// Read-only summaries on request from named contacts.
    Memorialise,
    /// Crypto-shred via D-189.
    Wipe,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LegacyTrigger {
    DeathCertificateUploaded,
    InactivityDays365,
    Manual,
}

/// Parse the `~/.harness/legacy.toml` body. The file shape is:
/// ```toml
/// policy = "lock"
/// trigger = "manual"
/// ```
/// Anything missing falls back to `Lock` / `Manual` — the safer
/// default.
pub fn parse_legacy_toml(body: &str) -> (LegacyPolicy, LegacyTrigger) {
    let mut policy = LegacyPolicy::Lock;
    let mut trigger = LegacyTrigger::Manual;
    for line in body.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("policy") {
            policy = parse_policy(rest).unwrap_or(LegacyPolicy::Lock);
        } else if let Some(rest) = trimmed.strip_prefix("trigger") {
            trigger = parse_trigger(rest).unwrap_or(LegacyTrigger::Manual);
        }
    }
    (policy, trigger)
}

fn parse_policy(rest: &str) -> Option<LegacyPolicy> {
    let value = rest
        .trim_start()
        .trim_start_matches('=')
        .trim()
        .trim_matches('"');
    match value {
        "lock" => Some(LegacyPolicy::Lock),
        "memorialise" => Some(LegacyPolicy::Memorialise),
        "wipe" => Some(LegacyPolicy::Wipe),
        _ => None,
    }
}

fn parse_trigger(rest: &str) -> Option<LegacyTrigger> {
    let value = rest
        .trim_start()
        .trim_start_matches('=')
        .trim()
        .trim_matches('"');
    match value {
        "death_certificate_uploaded" => Some(LegacyTrigger::DeathCertificateUploaded),
        "inactivity_days_365" => Some(LegacyTrigger::InactivityDays365),
        "manual" => Some(LegacyTrigger::Manual),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_body_falls_back_to_lock_manual() {
        assert_eq!(
            parse_legacy_toml(""),
            (LegacyPolicy::Lock, LegacyTrigger::Manual)
        );
    }

    #[test]
    fn explicit_memorialise_parses() {
        let (p, t) = parse_legacy_toml("policy = \"memorialise\"\ntrigger = \"manual\"");
        assert_eq!(p, LegacyPolicy::Memorialise);
        assert_eq!(t, LegacyTrigger::Manual);
    }

    #[test]
    fn unknown_policy_falls_back_to_lock() {
        let (p, _) = parse_legacy_toml("policy = \"other\"");
        assert_eq!(p, LegacyPolicy::Lock);
    }

    #[test]
    fn death_cert_trigger_parses() {
        let (_, t) = parse_legacy_toml("trigger = \"death_certificate_uploaded\"");
        assert_eq!(t, LegacyTrigger::DeathCertificateUploaded);
    }
}
