// IMPLEMENTS: D-184, D-382, D-402
//! Crisis intervention. The Character.AI 2026-01 settlement made it
//! mandatory to (1) detect crisis signals deterministically, (2) inject a
//! hotline directive in every concerning reply, and (3) escalate to a
//! guardian when the user has consented to that channel.
//!
//! Three responsibilities live here:
//! - [`detect`] — phrase-level detector for English and Korean. Heuristic
//!   only; pairs with the LLM's own refusal flow but can fire even when
//!   the model declines to answer.
//! - [`Hotline`] — per-region resource catalog the daemon splices into
//!   the prompt or the surface UI on every concerning reply (D-184).
//! - [`Escalation`] — guardian routing with three independent consent
//!   axes (parent / APS / school-official) per D-402.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CrisisLevel {
    None,
    /// Distress language present but no plan / means specified.
    Concern,
    /// Plan / means / timeline mentioned — supervisor escalation
    /// candidate per D-382.
    Imminent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DetectionHit {
    pub level: CrisisLevel,
    pub phrase: &'static str,
    pub byte_offset: usize,
}

const CONCERN_PHRASES: &[&str] = &[
    // English
    "i want to die",
    "i wish i was dead",
    "kill myself",
    "end it all",
    "no reason to live",
    "hurt myself",
    "self-harm",
    // Korean
    "죽고 싶",
    "자해",
    "끝내고 싶",
    "살기 싫",
];

const IMMINENT_PHRASES: &[&str] = &[
    // English — language signaling means / plan / timeline.
    "tonight i will",
    "i have a plan",
    "i bought pills",
    "i'm going to jump",
    "ready to die",
    // Korean
    "오늘 밤에",
    "약을 모았",
    "방법을 정했",
];

/// Scan the user's message for crisis phrases. Returns the highest level
/// observed plus every supporting hit (so the daemon can attach evidence
/// when escalating).
#[must_use]
pub fn detect(text: &str) -> (CrisisLevel, Vec<DetectionHit>) {
    let lower = text.to_lowercase();
    let mut hits = Vec::new();
    for phrase in IMMINENT_PHRASES {
        if let Some(idx) = lower.find(phrase) {
            hits.push(DetectionHit {
                level: CrisisLevel::Imminent,
                phrase,
                byte_offset: idx,
            });
        }
    }
    for phrase in CONCERN_PHRASES {
        if let Some(idx) = lower.find(phrase) {
            hits.push(DetectionHit {
                level: CrisisLevel::Concern,
                phrase,
                byte_offset: idx,
            });
        }
    }
    let level = hits
        .iter()
        .map(|h| h.level)
        .max()
        .unwrap_or(CrisisLevel::None);
    (level, hits)
}

/// Region-specific hotline. The daemon picks the right entry by user
/// locale or explicit `region` config.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hotline {
    pub region: &'static str,
    pub name: &'static str,
    pub phone: &'static str,
    pub url: &'static str,
}

const HOTLINES: &[Hotline] = &[
    Hotline {
        region: "US",
        name: "988 Suicide & Crisis Lifeline",
        phone: "988",
        url: "https://988lifeline.org/",
    },
    Hotline {
        region: "UK",
        name: "Samaritans",
        phone: "116 123",
        url: "https://www.samaritans.org/",
    },
    Hotline {
        region: "KR",
        name: "정신건강 위기상담전화",
        phone: "1577-0199",
        url: "https://www.ncmh.go.kr/",
    },
    Hotline {
        region: "JP",
        name: "TELL Lifeline",
        phone: "03-5774-0992",
        url: "https://telljp.com/",
    },
    Hotline {
        region: "DE",
        name: "Telefonseelsorge",
        phone: "0800 111 0 111",
        url: "https://www.telefonseelsorge.de/",
    },
    Hotline {
        region: "FR",
        name: "SOS Amitié",
        phone: "09 72 39 40 50",
        url: "https://www.sos-amitie.com/",
    },
    Hotline {
        region: "ES",
        name: "Teléfono de la Esperanza",
        phone: "717 003 717",
        url: "https://telefonodelaesperanza.org/",
    },
];

#[must_use]
pub fn hotline_for(region: &str) -> Option<&'static Hotline> {
    HOTLINES
        .iter()
        .find(|h| h.region.eq_ignore_ascii_case(region))
}

#[must_use]
pub fn all_hotlines() -> &'static [Hotline] {
    HOTLINES
}

/// Render a hotline message that gets injected into the assistant reply
/// whenever [`detect`] returns ≥ Concern. The text is intentionally short
/// — D-184 prefers a directive over a paragraph.
#[must_use]
pub fn render_inject(hotline: &Hotline) -> String {
    format!(
        "If you are in crisis, please contact {} ({}) — {}",
        hotline.name, hotline.phone, hotline.url
    )
}

/// Three independent consent axes per D-402. A guardian channel only
/// activates if the matching axis is explicitly true; absence defaults to
/// no contact (privacy-safe).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Consent {
    pub parent: bool,
    pub adult_protective_services: bool,
    pub school_official: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EscalationChannel {
    HotlineOnly,
    Parent,
    AdultProtectiveServices,
    SchoolOfficial,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EscalationPlan {
    pub level: CrisisLevel,
    pub channels: Vec<EscalationChannel>,
}

/// D-382: Imminent → escalate to every consented channel. Concern →
/// hotline only. None → empty plan.
#[must_use]
pub fn escalation_for(level: CrisisLevel, consent: &Consent) -> EscalationPlan {
    let mut channels = Vec::new();
    match level {
        CrisisLevel::None => {}
        CrisisLevel::Concern => channels.push(EscalationChannel::HotlineOnly),
        CrisisLevel::Imminent => {
            channels.push(EscalationChannel::HotlineOnly);
            if consent.parent {
                channels.push(EscalationChannel::Parent);
            }
            if consent.adult_protective_services {
                channels.push(EscalationChannel::AdultProtectiveServices);
            }
            if consent.school_official {
                channels.push(EscalationChannel::SchoolOfficial);
            }
        }
    }
    EscalationPlan { level, channels }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neutral_text_yields_none() {
        let (lvl, hits) = detect("how do I write a fizzbuzz?");
        assert_eq!(lvl, CrisisLevel::None);
        assert!(hits.is_empty());
    }

    #[test]
    fn english_concern_phrase_triggers_concern() {
        let (lvl, hits) = detect("I want to die today");
        assert_eq!(lvl, CrisisLevel::Concern);
        assert!(!hits.is_empty());
    }

    #[test]
    fn korean_concern_phrase_triggers_concern() {
        let (lvl, _) = detect("정말 죽고 싶다.");
        assert_eq!(lvl, CrisisLevel::Concern);
    }

    #[test]
    fn imminent_phrase_promotes_to_imminent_even_with_concern_hits() {
        let (lvl, _) = detect("I want to die. Tonight I will end it.");
        assert_eq!(lvl, CrisisLevel::Imminent);
    }

    #[test]
    fn detection_is_case_insensitive() {
        let (lvl, _) = detect("I WISH I WAS DEAD");
        assert_eq!(lvl, CrisisLevel::Concern);
    }

    #[test]
    fn hotline_lookup_is_region_case_insensitive() {
        assert!(hotline_for("us").is_some());
        assert!(hotline_for("KR").is_some());
        assert!(hotline_for("xx").is_none());
    }

    #[test]
    fn render_inject_includes_phone_and_url() {
        let h = hotline_for("US").unwrap();
        let msg = render_inject(h);
        assert!(msg.contains("988"));
        assert!(msg.contains("988lifeline.org"));
    }

    #[test]
    fn escalation_none_yields_empty_plan() {
        let plan = escalation_for(CrisisLevel::None, &Consent::default());
        assert!(plan.channels.is_empty());
    }

    #[test]
    fn escalation_concern_yields_only_hotline() {
        let plan = escalation_for(
            CrisisLevel::Concern,
            &Consent {
                parent: true,
                ..Consent::default()
            },
        );
        assert_eq!(plan.channels, vec![EscalationChannel::HotlineOnly]);
    }

    #[test]
    fn escalation_imminent_includes_every_consented_channel() {
        let plan = escalation_for(
            CrisisLevel::Imminent,
            &Consent {
                parent: true,
                adult_protective_services: true,
                school_official: true,
            },
        );
        assert_eq!(
            plan.channels,
            vec![
                EscalationChannel::HotlineOnly,
                EscalationChannel::Parent,
                EscalationChannel::AdultProtectiveServices,
                EscalationChannel::SchoolOfficial,
            ]
        );
    }

    #[test]
    fn escalation_imminent_without_consent_falls_back_to_hotline_only() {
        let plan = escalation_for(CrisisLevel::Imminent, &Consent::default());
        assert_eq!(plan.channels, vec![EscalationChannel::HotlineOnly]);
    }

    #[test]
    fn hotlines_list_covers_seven_launch_locales_at_minimum() {
        assert!(all_hotlines().len() >= 7);
        for region in ["US", "UK", "KR", "JP", "DE", "FR", "ES"] {
            assert!(hotline_for(region).is_some(), "missing {region}");
        }
    }
}
