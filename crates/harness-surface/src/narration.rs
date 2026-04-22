// IMPLEMENTS: D-273
//! ProgressUpdate carries the *intent* line. The surface prints both
//! "what" (the action being taken) and "why" (the goal it serves) —
//! a `ProgressUpdate` lacking the why field is rejected by
//! `validate_narration` so it never reaches the user.

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProgressNarration {
    pub what: String,
    pub why: String,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum NarrationError {
    #[error("ProgressUpdate missing 'what' line")]
    MissingWhat,
    #[error("ProgressUpdate missing 'why' line")]
    MissingWhy,
}

pub fn validate_narration(n: &ProgressNarration) -> Result<(), NarrationError> {
    if n.what.trim().is_empty() {
        return Err(NarrationError::MissingWhat);
    }
    if n.why.trim().is_empty() {
        return Err(NarrationError::MissingWhy);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_narration_validates() {
        let n = ProgressNarration {
            what: "Editing config.toml".into(),
            why: "to apply the new model selection".into(),
        };
        assert!(validate_narration(&n).is_ok());
    }

    #[test]
    fn missing_why_rejected() {
        let n = ProgressNarration {
            what: "Doing something".into(),
            why: "  ".into(),
        };
        assert_eq!(validate_narration(&n), Err(NarrationError::MissingWhy));
    }

    #[test]
    fn missing_what_rejected() {
        let n = ProgressNarration {
            what: String::new(),
            why: "x".into(),
        };
        assert_eq!(validate_narration(&n), Err(NarrationError::MissingWhat));
    }
}
