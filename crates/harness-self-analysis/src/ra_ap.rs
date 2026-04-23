// IMPLEMENTS: D-354
//! `ra_ap_*` self-analysis. The Harness daemon can load
//! `rust-analyzer`'s analysis crates in-process and run a small set
//! of Rust-specific checks against its own source tree. Output is a
//! `SelfAnalysisFinding` list; the gate decides whether any finding
//! blocks a release.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RaApAnalysis {
    UnusedDependency,
    PubItemMissingDoc,
    LargeEnumVariant,
    UnreachableCode,
    SuspiciousLifetime,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelfAnalysisFinding {
    pub analysis: RaApAnalysis,
    pub crate_name: String,
    pub item_path: String,
}

#[must_use]
pub fn registered_ra_ap_analyses() -> &'static [RaApAnalysis] {
    use RaApAnalysis::*;
    const ALL: &[RaApAnalysis] = &[
        UnusedDependency,
        PubItemMissingDoc,
        LargeEnumVariant,
        UnreachableCode,
        SuspiciousLifetime,
    ];
    ALL
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SelfAnalysisVerdict {
    Pass,
    /// Listed in the order discovered.
    Fail(Vec<SelfAnalysisFinding>),
}

#[must_use]
pub fn classify_findings(findings: Vec<SelfAnalysisFinding>) -> SelfAnalysisVerdict {
    if findings.is_empty() {
        SelfAnalysisVerdict::Pass
    } else {
        SelfAnalysisVerdict::Fail(findings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn five_analyses_listed() {
        assert_eq!(registered_ra_ap_analyses().len(), 5);
    }

    #[test]
    fn empty_findings_pass() {
        assert_eq!(classify_findings(vec![]), SelfAnalysisVerdict::Pass);
    }

    #[test]
    fn any_finding_fails() {
        let v = classify_findings(vec![SelfAnalysisFinding {
            analysis: RaApAnalysis::UnusedDependency,
            crate_name: "harness-foo".into(),
            item_path: "Cargo.toml".into(),
        }]);
        match v {
            SelfAnalysisVerdict::Fail(f) => assert_eq!(f.len(), 1),
            SelfAnalysisVerdict::Pass => panic!("expected fail"),
        }
    }
}
