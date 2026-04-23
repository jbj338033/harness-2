// IMPLEMENTS: D-427
//! GPAI Code of Practice safety-info parser. Each block is a small
//! TOML-ish format:
//! ```text
//! capability = "..."
//! mitigations = "..."
//! red_team_passes = N
//! ```
//! We extract `(capability, mitigations, red_team_passes)`. Missing
//! fields fall back to empty / 0 — the gate that consumes the entry
//! decides whether to fail.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GpaiSafetyEntry {
    pub capability: String,
    pub mitigations: String,
    pub red_team_passes: u32,
}

#[must_use]
pub fn parse_gpai_safety_block(body: &str) -> GpaiSafetyEntry {
    let mut entry = GpaiSafetyEntry {
        capability: String::new(),
        mitigations: String::new(),
        red_team_passes: 0,
    };
    for line in body.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("capability") {
            entry.capability = strip_value(rest);
        } else if let Some(rest) = trimmed.strip_prefix("mitigations") {
            entry.mitigations = strip_value(rest);
        } else if let Some(rest) = trimmed.strip_prefix("red_team_passes") {
            let raw = strip_value(rest);
            entry.red_team_passes = raw.parse().unwrap_or(0);
        }
    }
    entry
}

fn strip_value(rest: &str) -> String {
    rest.trim_start()
        .trim_start_matches('=')
        .trim()
        .trim_matches('"')
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_block_parses() {
        let e = parse_gpai_safety_block(
            "capability = \"code-gen\"\nmitigations = \"sandbox\"\nred_team_passes = 3",
        );
        assert_eq!(e.capability, "code-gen");
        assert_eq!(e.mitigations, "sandbox");
        assert_eq!(e.red_team_passes, 3);
    }

    #[test]
    fn missing_field_defaults_to_empty() {
        let e = parse_gpai_safety_block("capability = \"x\"");
        assert_eq!(e.mitigations, "");
        assert_eq!(e.red_team_passes, 0);
    }

    #[test]
    fn malformed_passes_falls_back_to_zero() {
        let e = parse_gpai_safety_block("red_team_passes = NaN");
        assert_eq!(e.red_team_passes, 0);
    }
}
