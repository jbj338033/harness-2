// IMPLEMENTS: D-256
//! Five canonical incident-skill role presets. Each maps to a SKILL
//! preset already shipped by D-249 — this enum is the contract that
//! lets the preset registry resolve it.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IncidentRolePreset {
    IncidentCommander,
    OpsLead,
    CommsLead,
    PostmortemScribe,
    SreInvestigator,
}

#[must_use]
pub fn all_incident_presets() -> &'static [IncidentRolePreset] {
    use IncidentRolePreset::*;
    const ALL: &[IncidentRolePreset] = &[
        IncidentCommander,
        OpsLead,
        CommsLead,
        PostmortemScribe,
        SreInvestigator,
    ];
    ALL
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exactly_five_presets() {
        assert_eq!(all_incident_presets().len(), 5);
    }

    #[test]
    fn includes_commander_and_scribe() {
        let all = all_incident_presets();
        assert!(all.contains(&IncidentRolePreset::IncidentCommander));
        assert!(all.contains(&IncidentRolePreset::PostmortemScribe));
    }
}
