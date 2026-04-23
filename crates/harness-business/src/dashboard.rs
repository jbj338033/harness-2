// IMPLEMENTS: D-410
//! Quarterly transparency dashboard. Pinned section list so a
//! quarter can't ship without each one filled in.

use serde::{Deserialize, Serialize};

pub const QUARTERLY_DASHBOARD_SECTIONS: &[&str] = &[
    "revenue",
    "active_users",
    "expense",
    "incidents",
    "compliance_findings",
    "carbon_footprint",
    "open_contributions",
    "next_quarter_roadmap",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuarterlyDashboard {
    pub year: u16,
    pub quarter: u8,
    pub sections: Vec<(String, String)>,
}

#[must_use]
pub fn dashboard_section_count() -> usize {
    QUARTERLY_DASHBOARD_SECTIONS.len()
}

impl QuarterlyDashboard {
    /// Returns names of sections still missing a body.
    #[must_use]
    pub fn missing_sections(&self) -> Vec<&'static str> {
        QUARTERLY_DASHBOARD_SECTIONS
            .iter()
            .copied()
            .filter(|name| {
                !self
                    .sections
                    .iter()
                    .any(|(k, body)| k == *name && !body.trim().is_empty())
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eight_pinned_sections() {
        assert_eq!(dashboard_section_count(), 8);
    }

    #[test]
    fn empty_dashboard_lists_all_missing() {
        let d = QuarterlyDashboard {
            year: 2026,
            quarter: 2,
            sections: vec![],
        };
        assert_eq!(d.missing_sections().len(), 8);
    }

    #[test]
    fn filled_section_drops_from_missing() {
        let d = QuarterlyDashboard {
            year: 2026,
            quarter: 2,
            sections: vec![("revenue".into(), "Q2 +12%".into())],
        };
        assert!(!d.missing_sections().contains(&"revenue"));
    }

    #[test]
    fn whitespace_body_still_counts_as_missing() {
        let d = QuarterlyDashboard {
            year: 2026,
            quarter: 2,
            sections: vec![("revenue".into(), "  ".into())],
        };
        assert!(d.missing_sections().contains(&"revenue"));
    }
}
