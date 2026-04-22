// IMPLEMENTS: D-403
//! Vulnerability disclosure surface — the README line, the
//! `harness whoami` row schema, and a pointer to the issue template
//! for vulnerable-user reports.

use crate::profile::VulnerabilityLevel;
use serde::{Deserialize, Serialize};

pub const DISCLOSURE_README_LINE: &str = "Harness ships a vulnerable-users protection surface (`harness-vulnerable`). Reports: see VULNERABLE_USER_REPORT.md.";

pub const DISCLOSURE_VULN_ISSUE_TEMPLATE_PATH: &str =
    ".github/ISSUE_TEMPLATE/vulnerable_user_report.yml";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VulnWhoAmIRow {
    pub level: VulnerabilityLevel,
    pub signals_active: bool,
    pub guardian_consent_axis: Option<&'static str>,
    pub disable_path: &'static str,
}

#[must_use]
pub fn vuln_whoami_row(
    level: VulnerabilityLevel,
    signals_active: bool,
    guardian_axis: Option<&'static str>,
) -> VulnWhoAmIRow {
    VulnWhoAmIRow {
        level,
        signals_active,
        guardian_consent_axis: guardian_axis,
        disable_path: "harness vulnerable disable",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whoami_advertises_disable_path() {
        let row = vuln_whoami_row(VulnerabilityLevel::Elevated, true, None);
        assert_eq!(row.disable_path, "harness vulnerable disable");
    }

    #[test]
    fn issue_template_path_points_under_github() {
        assert!(DISCLOSURE_VULN_ISSUE_TEMPLATE_PATH.starts_with(".github/"));
    }

    #[test]
    fn readme_line_points_to_report_doc() {
        assert!(DISCLOSURE_README_LINE.contains("VULNERABLE_USER_REPORT.md"));
    }
}
