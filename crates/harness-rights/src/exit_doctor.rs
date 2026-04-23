// IMPLEMENTS: D-423
//! `harness doctor --exit` checklist. Runs the canonical departure
//! steps so a user can leave the product cleanly: export Agent
//! Trace, export memory, export consent VC, retire keys.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExitDoctorStep {
    ExportAgentTrace,
    ExportMemory,
    ExportConsentVc,
    RetireKeys,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExitDoctorReport {
    pub completed: Vec<ExitDoctorStep>,
    pub remaining: Vec<ExitDoctorStep>,
}

#[must_use]
pub fn run_exit_doctor(completed: &[ExitDoctorStep]) -> ExitDoctorReport {
    use ExitDoctorStep::*;
    const ALL: [ExitDoctorStep; 4] = [ExportAgentTrace, ExportMemory, ExportConsentVc, RetireKeys];
    let remaining: Vec<ExitDoctorStep> = ALL
        .iter()
        .copied()
        .filter(|s| !completed.contains(s))
        .collect();
    ExitDoctorReport {
        completed: completed.to_vec(),
        remaining,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_done_yields_full_remaining() {
        let r = run_exit_doctor(&[]);
        assert_eq!(r.remaining.len(), 4);
        assert!(r.completed.is_empty());
    }

    #[test]
    fn partial_done_filtered_out() {
        let r = run_exit_doctor(&[ExitDoctorStep::ExportAgentTrace]);
        assert_eq!(r.remaining.len(), 3);
        assert!(!r.remaining.contains(&ExitDoctorStep::ExportAgentTrace));
    }

    #[test]
    fn all_done_remaining_empty() {
        use ExitDoctorStep::*;
        let r = run_exit_doctor(&[ExportAgentTrace, ExportMemory, ExportConsentVc, RetireKeys]);
        assert!(r.remaining.is_empty());
    }
}
