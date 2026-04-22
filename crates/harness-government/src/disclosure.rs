// IMPLEMENTS: D-389
//! Citizen-facing disclosure with the LEP (Limited English Proficient)
//! language matrix. Even after EO 13166's revocation, Title VI of the
//! 1964 Civil Rights Act + state law + EU + Korea requirements still
//! require multi-language access. We pin the canonical matrix here so
//! the surface can pick the right line.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CitizenRegime {
    UsTitleVi,
    UsState,
    Eu,
    Kr,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CitizenDisclosure {
    pub regime: CitizenRegime,
    pub languages: Vec<&'static str>,
    pub line: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LepMatrixRow {
    pub regime: CitizenRegime,
    pub bcp47: &'static str,
    pub line: &'static str,
}

const MATRIX: &[LepMatrixRow] = &[
    LepMatrixRow {
        regime: CitizenRegime::UsTitleVi,
        bcp47: "en",
        line: "Free language assistance is available. Ask the agent for an interpreter.",
    },
    LepMatrixRow {
        regime: CitizenRegime::UsTitleVi,
        bcp47: "es",
        line: "La asistencia lingüística gratuita está disponible. Pida un intérprete al agente.",
    },
    LepMatrixRow {
        regime: CitizenRegime::Eu,
        bcp47: "en",
        line: "This service uses an automated AI system. You may request human review.",
    },
    LepMatrixRow {
        regime: CitizenRegime::Eu,
        bcp47: "fr",
        line: "Ce service utilise un système d'IA automatisé. Vous pouvez demander un examen humain.",
    },
    LepMatrixRow {
        regime: CitizenRegime::Eu,
        bcp47: "de",
        line: "Dieser Dienst nutzt ein automatisiertes KI-System. Sie können eine menschliche Überprüfung verlangen.",
    },
    LepMatrixRow {
        regime: CitizenRegime::Kr,
        bcp47: "ko",
        line: "이 서비스는 자동화된 AI 시스템을 사용합니다. 사람 검토를 요청할 수 있습니다.",
    },
    LepMatrixRow {
        regime: CitizenRegime::UsState,
        bcp47: "en",
        line: "An automated decision system was used. State law may grant you appeal rights.",
    },
];

#[must_use]
pub fn citizen_disclosure_for(regime: CitizenRegime, bcp47: &str) -> Option<&'static LepMatrixRow> {
    MATRIX
        .iter()
        .find(|r| r.regime == regime && r.bcp47 == bcp47)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_vi_spanish_line_present() {
        let r = citizen_disclosure_for(CitizenRegime::UsTitleVi, "es").unwrap();
        assert!(r.line.contains("intérprete"));
    }

    #[test]
    fn eu_german_line_has_umlaut() {
        let r = citizen_disclosure_for(CitizenRegime::Eu, "de").unwrap();
        assert!(r.line.contains("Überprüfung"));
    }

    #[test]
    fn kr_korean_line_present() {
        let r = citizen_disclosure_for(CitizenRegime::Kr, "ko").unwrap();
        assert!(r.line.contains("AI"));
    }

    #[test]
    fn unknown_combo_returns_none() {
        assert!(citizen_disclosure_for(CitizenRegime::Kr, "fr").is_none());
    }
}
