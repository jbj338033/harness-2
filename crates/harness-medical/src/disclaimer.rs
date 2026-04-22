// IMPLEMENTS: D-364
//! "Not medical advice" three-line guard. Medical-mode replies must
//! carry the disclaimer; if missing, we prepend it. Idempotent — a
//! reply that already includes it is left alone.

pub const MEDICAL_DISCLAIMER: &str = "[Not medical advice — informational only. Consult a licensed clinician for diagnosis or treatment.]";

#[must_use]
pub fn ensure_disclaimer(reply: &str) -> String {
    if reply.contains(MEDICAL_DISCLAIMER) {
        reply.to_string()
    } else {
        format!("{MEDICAL_DISCLAIMER}\n\n{reply}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disclaimer_prepended_when_missing() {
        let out = ensure_disclaimer("Symptom X is common.");
        assert!(out.starts_with(MEDICAL_DISCLAIMER));
    }

    #[test]
    fn disclaimer_idempotent() {
        let first = ensure_disclaimer("A");
        let second = ensure_disclaimer(&first);
        assert_eq!(first, second);
    }
}
