// IMPLEMENTS: D-324
//! Stateright Turn Phase model-check spec. The product space is
//! `TurnPhase × CrashTime`; every cell must be reachable in the
//! generated state graph or we have lost coverage.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnPhase {
    UserMessageWritten,
    AssistantPlaceholderWritten,
    StreamingTokens,
    ToolCallEmitted,
    ToolResultRecorded,
    ChatReentered,
    DoneRecorded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CrashTime {
    BeforePhase,
    MidPhase,
    AfterPhase,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ModelCheckCell {
    pub phase: TurnPhase,
    pub crash: CrashTime,
}

#[must_use]
pub fn all_cells() -> Vec<ModelCheckCell> {
    use CrashTime::*;
    use TurnPhase::*;
    let phases = [
        UserMessageWritten,
        AssistantPlaceholderWritten,
        StreamingTokens,
        ToolCallEmitted,
        ToolResultRecorded,
        ChatReentered,
        DoneRecorded,
    ];
    let crashes = [BeforePhase, MidPhase, AfterPhase];
    let mut out = Vec::with_capacity(phases.len() * crashes.len());
    for phase in phases {
        for crash in crashes {
            out.push(ModelCheckCell { phase, crash });
        }
    }
    out
}

#[must_use]
pub fn cell_count() -> usize {
    all_cells().len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn product_space_is_seven_phases_times_three_crash_times() {
        assert_eq!(cell_count(), 21);
    }

    #[test]
    fn every_cell_is_unique() {
        let mut sorted = all_cells();
        sorted.sort_by_key(|c| (c.phase, c.crash));
        sorted.dedup();
        assert_eq!(sorted.len(), 21);
    }

    #[test]
    fn done_recorded_after_phase_present() {
        let cells = all_cells();
        assert!(cells.iter().any(|c| matches!(
            (c.phase, c.crash),
            (TurnPhase::DoneRecorded, CrashTime::AfterPhase)
        )));
    }
}
