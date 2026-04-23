// IMPLEMENTS: D-249
//! Nine canonical role presets — MetaGPT (PM/Architect/Engineer/QA) +
//! ChatDev (CEO/CPO/CTO) + gstack (CEO/Eng/DesignReview) deduplicated.
//! CEO appears in both sets; we deduplicate to a single Strategy role.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TeamRolePreset {
    ProductManager,
    Architect,
    Engineer,
    Qa,
    /// Ex-CEO/CPO consolidated.
    Strategy,
    Cto,
    DesignReview,
    /// Operations / coordination role from ChatDev.
    Coordinator,
    Researcher,
}

#[must_use]
pub fn all_team_role_presets() -> &'static [TeamRolePreset] {
    use TeamRolePreset::*;
    const ALL: &[TeamRolePreset] = &[
        ProductManager,
        Architect,
        Engineer,
        Qa,
        Strategy,
        Cto,
        DesignReview,
        Coordinator,
        Researcher,
    ];
    ALL
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nine_role_presets() {
        assert_eq!(all_team_role_presets().len(), 9);
    }

    #[test]
    fn includes_engineer_qa_designreview() {
        let all = all_team_role_presets();
        assert!(all.contains(&TeamRolePreset::Engineer));
        assert!(all.contains(&TeamRolePreset::Qa));
        assert!(all.contains(&TeamRolePreset::DesignReview));
    }
}
