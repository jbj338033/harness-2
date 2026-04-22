// IMPLEMENTS: D-376, D-377, D-378, D-379, D-380, D-381, D-382
//! Education mode (axis off by default).
//!
//! - [`meta`] — D-376: educator skill metadata (9 fields).
//! - [`socratic`] — D-377: Socratic loop with a Vygotsky ZPD hint
//!   scaler (`hint_level` 0–4).
//! - [`coppa`] — D-378: under-13 default block (the 2026-04-22
//!   compliance milestone).
//! - [`learner`] — D-379: per-learner FSRS (Free Spaced Repetition
//!   Scheduler) record + AES-256 spec.
//! - [`stream`] — D-380: ed25519-signed learner event stream
//!   (no central server — every event is signed locally).
//! - [`provenance`] — D-381: provenance over detection. We refuse to
//!   bundle classifier-style "AI detection" (Stanford studies show
//!   ~61% false-positive on ESL writing) and instead emit author
//!   provenance metadata.
//! - [`crisis`] — D-382: crisis protocol + supervisor escalation
//!   (Character.AI 2026-01 settlement was the forcing function).

pub mod coppa;
pub mod crisis;
pub mod learner;
pub mod meta;
pub mod provenance;
pub mod socratic;
pub mod stream;

pub use coppa::{CoppaDecision, CoppaError, evaluate_coppa};
pub use crisis::{CrisisLevel, CrisisOutcome, escalate};
pub use learner::{FsrsRecord, LearnerCardState};
pub use meta::{
    AgeBand, AnswerPolicy, CrisisProtocolMode, EducatorSkillMeta, LearningMode,
    SupervisorVisibility,
};
pub use provenance::{AiDetectionRefusal, AuthorProvenance, refuse_ai_detection};
pub use socratic::{HintLevel, NextHint, scale_hint};
pub use stream::{LearnerEvent, SignedEvent, sign_event, verify_event};
