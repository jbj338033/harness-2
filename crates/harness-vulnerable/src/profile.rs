// IMPLEMENTS: D-397
//! `VulnerabilityProfile` — four orthogonal bands. Unknown is always
//! treated as *Elevated* so the safer behaviour ships first.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgeBand {
    Unknown,
    Child,
    Adolescent,
    Adult,
    Older,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CognitiveBand {
    Unknown,
    Typical,
    Atypical,
    Impaired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GuardianBand {
    Unknown,
    None,
    Designated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrustBand {
    Unknown,
    Low,
    Normal,
    High,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VulnerabilityLevel {
    Standard,
    /// Anything that resolves to "we don't know enough" or has one
    /// raised band lifts to here.
    Elevated,
    /// Multiple raised bands — `L_vulnerable` lock applies (D-399).
    LVulnerable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct VulnerabilityProfile {
    pub age: AgeBand,
    pub cognitive: CognitiveBand,
    pub guardian: GuardianBand,
    pub trust: TrustBand,
}

impl VulnerabilityProfile {
    #[must_use]
    pub fn level(&self) -> VulnerabilityLevel {
        let raised = self.raised_band_count();
        if raised >= 2 {
            VulnerabilityLevel::LVulnerable
        } else if raised == 1 || self.has_unknown() {
            VulnerabilityLevel::Elevated
        } else {
            VulnerabilityLevel::Standard
        }
    }

    fn raised_band_count(&self) -> u8 {
        let mut n = 0u8;
        if matches!(
            self.age,
            AgeBand::Child | AgeBand::Adolescent | AgeBand::Older
        ) {
            n += 1;
        }
        if matches!(
            self.cognitive,
            CognitiveBand::Atypical | CognitiveBand::Impaired
        ) {
            n += 1;
        }
        if matches!(self.guardian, GuardianBand::Designated) {
            n += 1;
        }
        if matches!(self.trust, TrustBand::Low) {
            n += 1;
        }
        n
    }

    fn has_unknown(&self) -> bool {
        matches!(self.age, AgeBand::Unknown)
            || matches!(self.cognitive, CognitiveBand::Unknown)
            || matches!(self.guardian, GuardianBand::Unknown)
            || matches!(self.trust, TrustBand::Unknown)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(age: AgeBand, cog: CognitiveBand, g: GuardianBand, t: TrustBand) -> VulnerabilityProfile {
        VulnerabilityProfile {
            age,
            cognitive: cog,
            guardian: g,
            trust: t,
        }
    }

    #[test]
    fn unknown_anywhere_is_elevated() {
        let v = p(
            AgeBand::Unknown,
            CognitiveBand::Typical,
            GuardianBand::None,
            TrustBand::Normal,
        );
        assert_eq!(v.level(), VulnerabilityLevel::Elevated);
    }

    #[test]
    fn fully_typical_is_standard() {
        let v = p(
            AgeBand::Adult,
            CognitiveBand::Typical,
            GuardianBand::None,
            TrustBand::Normal,
        );
        assert_eq!(v.level(), VulnerabilityLevel::Standard);
    }

    #[test]
    fn two_raised_bands_lift_to_l_vulnerable() {
        let v = p(
            AgeBand::Child,
            CognitiveBand::Impaired,
            GuardianBand::None,
            TrustBand::Normal,
        );
        assert_eq!(v.level(), VulnerabilityLevel::LVulnerable);
    }
}
