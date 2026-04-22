// IMPLEMENTS: D-275
//! WCAG 2.2 AA + EN 301 549 v4.1.1 release blockers — automated CI
//! checks. Real DOM auditing happens in the surface tests; this
//! module enumerates the rules and produces the verdict object the
//! gate consumes.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccessibilityRule {
    /// WCAG 1.4.3 / EN 301 549 9.1.4.3 — text contrast.
    Contrast,
    /// WCAG 2.4.7 — focus visible.
    FocusVisible,
    /// WCAG 4.1.2 — name/role/value for custom widgets.
    NameRoleValue,
    /// WCAG 1.3.1 — landmarks.
    Landmarks,
    /// WCAG 3.3.7 (2.2 new) — redundant entry.
    RedundantEntry,
    /// EN 301 549 9.6 — closed captions track present.
    Captions,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccessibilityFinding {
    pub rule: AccessibilityRule,
    pub locator: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AccessibilityVerdict {
    Pass,
    Fail(Vec<AccessibilityFinding>),
}

#[must_use]
pub fn scan_for_a11y(findings: Vec<AccessibilityFinding>) -> AccessibilityVerdict {
    if findings.is_empty() {
        AccessibilityVerdict::Pass
    } else {
        AccessibilityVerdict::Fail(findings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_findings_pass() {
        assert_eq!(scan_for_a11y(vec![]), AccessibilityVerdict::Pass);
    }

    #[test]
    fn any_finding_fails_release() {
        let f = AccessibilityFinding {
            rule: AccessibilityRule::Contrast,
            locator: "button.primary".into(),
            message: "ratio 3.1:1 below 4.5:1".into(),
        };
        match scan_for_a11y(vec![f]) {
            AccessibilityVerdict::Fail(v) => assert_eq!(v.len(), 1),
            AccessibilityVerdict::Pass => panic!("expected failure"),
        }
    }
}
