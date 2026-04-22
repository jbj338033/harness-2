// IMPLEMENTS: D-376
//! Educator skill metadata — exactly the 9 fields D-376 fixes. Skills
//! that omit any field are rejected by `EducatorSkillMeta::validate`.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgeBand {
    Under13,
    Teen13to17,
    Adult,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LearningMode {
    Socratic,
    GuidedPractice,
    DirectInstruction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnswerPolicy {
    /// Never reveal the final answer — only hints.
    HintsOnly,
    /// Reveal answer after N attempts (handled by surface).
    AnswerAfterAttempts,
    /// Always show answer (homework-help style).
    AlwaysAnswer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SupervisorVisibility {
    None,
    SummaryOnly,
    FullTranscript,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CrisisProtocolMode {
    Off,
    HotlineOnly,
    HotlinePlusGuardian,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EducatorSkillMeta {
    pub role: String,
    pub audience: String,
    pub learning_mode: LearningMode,
    pub answer_policy: AnswerPolicy,
    pub age_band: AgeBand,
    pub subject_tags: Vec<String>,
    pub mastery_tracking: bool,
    pub supervisor_visibility: SupervisorVisibility,
    pub crisis_protocol: CrisisProtocolMode,
}

impl EducatorSkillMeta {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.role.trim().is_empty() {
            return Err("educator skill: role must not be empty");
        }
        if self.audience.trim().is_empty() {
            return Err("educator skill: audience must not be empty");
        }
        if self.subject_tags.is_empty() {
            return Err("educator skill: subject_tags must not be empty");
        }
        if matches!(self.age_band, AgeBand::Under13)
            && matches!(self.crisis_protocol, CrisisProtocolMode::Off)
        {
            return Err("educator skill: under-13 requires crisis protocol");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta() -> EducatorSkillMeta {
        EducatorSkillMeta {
            role: "math tutor".into(),
            audience: "8th grade".into(),
            learning_mode: LearningMode::Socratic,
            answer_policy: AnswerPolicy::HintsOnly,
            age_band: AgeBand::Teen13to17,
            subject_tags: vec!["algebra".into()],
            mastery_tracking: true,
            supervisor_visibility: SupervisorVisibility::SummaryOnly,
            crisis_protocol: CrisisProtocolMode::HotlinePlusGuardian,
        }
    }

    #[test]
    fn full_meta_validates() {
        assert!(meta().validate().is_ok());
    }

    #[test]
    fn empty_subject_tags_rejected() {
        let mut m = meta();
        m.subject_tags.clear();
        assert!(m.validate().is_err());
    }

    #[test]
    fn under_13_without_crisis_protocol_rejected() {
        let mut m = meta();
        m.age_band = AgeBand::Under13;
        m.crisis_protocol = CrisisProtocolMode::Off;
        assert!(m.validate().is_err());
    }
}
