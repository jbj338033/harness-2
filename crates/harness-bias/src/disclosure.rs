// IMPLEMENTS: D-306
//! README disclosure — three pinned lines. Their job is to make it
//! unambiguous that we sit *outside* EU AI Act Annex III's
//! high-risk categories: employment / credit / public-safety
//! decisioning, biometric identification, and "deep chatbot"
//! deployment without disclosure are all explicitly excluded.

pub const README_DISCLOSURE_LINE_COUNT: usize = 3;

pub const README_DISCLOSURE_LINES: [&str; 3] = [
    "Harness is NOT a decisioning system for employment, credit, or public-safety triage. Do not deploy it as the final decision-maker in those flows — it lacks the validation, explainability, and audit trail those domains require under EU AI Act Annex III, NYC LL144, and Colorado SB24-205.",
    "Harness does NOT perform biometric identification or categorisation. Camera and microphone access, when granted, are scoped to the active session and never used to identify individuals.",
    "Harness is not a 'deep chatbot' product. Every reply that crosses an outbound channel carries an explicit AI-disclosure line. This document is informational and is not legal advice.",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn three_lines_present() {
        assert_eq!(README_DISCLOSURE_LINES.len(), README_DISCLOSURE_LINE_COUNT);
    }

    #[test]
    fn first_line_excludes_decisioning() {
        assert!(README_DISCLOSURE_LINES[0].contains("decision"));
    }

    #[test]
    fn final_line_disclaims_legal_advice() {
        assert!(README_DISCLOSURE_LINES[2].contains("not legal advice"));
    }
}
