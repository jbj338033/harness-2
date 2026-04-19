#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Principle {
    EphemeralAgents,

    PlanAsContract,

    EvidenceRequired,

    QualityGate,

    DeclaredScope,
}

impl Principle {
    #[must_use]
    pub const fn all() -> &'static [Principle] {
        &[
            Principle::EphemeralAgents,
            Principle::PlanAsContract,
            Principle::EvidenceRequired,
            Principle::QualityGate,
            Principle::DeclaredScope,
        ]
    }

    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Principle::EphemeralAgents => "ephemeral",
            Principle::PlanAsContract => "plan-as-contract",
            Principle::EvidenceRequired => "evidence",
            Principle::QualityGate => "quality-gate",
            Principle::DeclaredScope => "scope",
        }
    }

    #[must_use]
    pub const fn instruction(self) -> &'static str {
        match self {
            Principle::EphemeralAgents => {
                "You are ephemeral. Do not rely on context memory. \
                 Record all durable state to the filesystem and database. \
                 A fresh agent must be able to resume your work by reading \
                 files alone."
            }
            Principle::PlanAsContract => {
                "The plan defines your scope. Do not perform work outside \
                 it. Do not revise locked decisions. Record any deviation \
                 explicitly."
            }
            Principle::EvidenceRequired => {
                "Never claim completion without evidence. \"should work\" \
                 and \"probably fine\" are failures. Provide test results, \
                 build output, or actual execution proof. Verify the GOAL, \
                 not task completion."
            }
            Principle::QualityGate => {
                "Never commit code that fails tests, lint, or type checks. \
                 Quality gates precede every commit. Failed work is fixed \
                 or discarded — never accumulated."
            }
            Principle::DeclaredScope => {
                "Modify only files declared in the plan. Destructive \
                 commands are forbidden. External system changes require \
                 explicit authorization. When uncertain, ask."
            }
        }
    }
}

impl std::fmt::Display for Principle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.id())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_principles_present() {
        assert_eq!(Principle::all().len(), 5);
    }

    #[test]
    fn ids_are_unique() {
        let ids: Vec<_> = Principle::all().iter().map(|p| p.id()).collect();
        let unique: std::collections::HashSet<_> = ids.iter().collect();
        assert_eq!(ids.len(), unique.len());
    }

    #[test]
    fn instructions_are_non_empty() {
        for p in Principle::all() {
            assert!(!p.instruction().is_empty());
            assert!(!p.id().is_empty());
        }
    }

    #[test]
    fn display_matches_id() {
        for p in Principle::all() {
            assert_eq!(format!("{p}"), p.id());
        }
    }
}
