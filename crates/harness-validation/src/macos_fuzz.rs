// IMPLEMENTS: D-193
//! macOS sandbox behavioural fuzz matrix. Each cell records the
//! result of a fuzz pass on a specific OS version × surface
//! combination so a deny-regression on one row trips the gate.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MacOsFuzzVerdict {
    AllDenied,
    /// One or more fuzz inputs were not denied.
    HoleFound,
    /// Fuzz could not be executed in this cell.
    InfraSkip,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MacOsFuzzCell {
    pub os_version: String,
    pub surface: String,
    pub verdict: MacOsFuzzVerdict,
    pub fuzz_run_id: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MacOsFuzzMatrix {
    pub cells: Vec<MacOsFuzzCell>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MacOsFuzzOutcome {
    Pass,
    Fail(Vec<MacOsFuzzCell>),
}

#[must_use]
pub fn evaluate_fuzz(matrix: &MacOsFuzzMatrix) -> MacOsFuzzOutcome {
    let bad: Vec<MacOsFuzzCell> = matrix
        .cells
        .iter()
        .filter(|c| matches!(c.verdict, MacOsFuzzVerdict::HoleFound))
        .cloned()
        .collect();
    if bad.is_empty() {
        MacOsFuzzOutcome::Pass
    } else {
        MacOsFuzzOutcome::Fail(bad)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cell(os: &str, surface: &str, v: MacOsFuzzVerdict) -> MacOsFuzzCell {
        MacOsFuzzCell {
            os_version: os.into(),
            surface: surface.into(),
            verdict: v,
            fuzz_run_id: "run-1".into(),
        }
    }

    #[test]
    fn all_denied_passes() {
        let m = MacOsFuzzMatrix {
            cells: vec![
                cell("14.6", "fs", MacOsFuzzVerdict::AllDenied),
                cell("15.2", "net", MacOsFuzzVerdict::AllDenied),
            ],
        };
        assert_eq!(evaluate_fuzz(&m), MacOsFuzzOutcome::Pass);
    }

    #[test]
    fn infra_skip_does_not_fail_gate() {
        let m = MacOsFuzzMatrix {
            cells: vec![cell("13.0", "proc", MacOsFuzzVerdict::InfraSkip)],
        };
        assert_eq!(evaluate_fuzz(&m), MacOsFuzzOutcome::Pass);
    }

    #[test]
    fn any_hole_fails() {
        let m = MacOsFuzzMatrix {
            cells: vec![
                cell("14.6", "fs", MacOsFuzzVerdict::AllDenied),
                cell("15.2", "fs", MacOsFuzzVerdict::HoleFound),
            ],
        };
        match evaluate_fuzz(&m) {
            MacOsFuzzOutcome::Fail(c) => assert_eq!(c.len(), 1),
            MacOsFuzzOutcome::Pass => panic!("expected fail"),
        }
    }
}
