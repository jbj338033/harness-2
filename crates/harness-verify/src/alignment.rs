// IMPLEMENTS: D-308
//! Trajectory Alignment Check — does the agent's `Speak` claim match what
//! the underlying `Act` results actually said? D-308 cites the Replit
//! incident: an agent reported "I deleted the production database, but
//! restored it from backup" while in reality nothing was restored. We
//! catch that class of false-completion claims by cross-referencing the
//! claim text with the most recent tool outcomes.

use serde::{Deserialize, Serialize};

/// One assertion the agent made in a `Speak` event. The detector parses
/// the claim out of the natural-language body (cheap heuristics — this is
/// a sanity check, not formal verification).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Claim {
    /// "I ran the tests and they pass."
    TestsPassing,
    /// "I built the project successfully."
    BuildSucceeded,
    /// "I wrote the file …".
    FileWritten { path: String },
    /// "Task complete" / "All done" — most generic claim.
    TaskComplete,
}

/// Result of a single tool call recorded in the events table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActOutcome {
    pub tool: String,
    pub exit_code: Option<i32>,
    pub stderr_excerpt: String,
    pub touched_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlignmentVerdict {
    Aligned,
    Misaligned { claim: Claim, reason: String },
    NoEvidence { claim: Claim },
}

/// Heuristic claim parser — looks for plain-English markers in the body.
/// `body` is whatever the assistant said in the most recent Speak event.
#[must_use]
pub fn parse_claims(body: &str) -> Vec<Claim> {
    let lower = body.to_ascii_lowercase();
    let mut out = Vec::new();
    if has_phrase(
        &lower,
        &[
            "tests pass",
            "all tests pass",
            "tests are passing",
            "test suite passes",
        ],
    ) {
        out.push(Claim::TestsPassing);
    }
    if has_phrase(
        &lower,
        &[
            "build succeeded",
            "build completed",
            "compiles cleanly",
            "builds successfully",
        ],
    ) {
        out.push(Claim::BuildSucceeded);
    }
    if let Some(path) = extract_written_path(&lower) {
        out.push(Claim::FileWritten { path });
    }
    if has_phrase(
        &lower,
        &[
            "task complete",
            "all done",
            "everything is done",
            "completed successfully",
        ],
    ) {
        out.push(Claim::TaskComplete);
    }
    out
}

fn has_phrase(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|n| haystack.contains(n))
}

fn extract_written_path(lower: &str) -> Option<String> {
    // Look for "wrote <path>" or "created <path>" — quote-delimited paths
    // are the only reliable form we can extract without a tokenizer.
    for marker in ["wrote ", "created ", "saved "] {
        if let Some(idx) = lower.find(marker) {
            let after = &lower[idx + marker.len()..];
            // Accept text inside backticks first
            if let Some(rest) = after.strip_prefix('`')
                && let Some(end) = rest.find('`')
            {
                return Some(rest[..end].to_string());
            }
            if let Some(rest) = after.strip_prefix('"')
                && let Some(end) = rest.find('"')
            {
                return Some(rest[..end].to_string());
            }
        }
    }
    None
}

/// Cross-check a claim against recent tool outcomes. The detector is
/// deliberately strict: missing evidence is reported separately so the
/// caller can decide whether to escalate or warn.
#[must_use]
pub fn check(claim: &Claim, outcomes: &[ActOutcome]) -> AlignmentVerdict {
    match claim {
        Claim::TestsPassing => match latest_tool(outcomes, &["test", "cargo.test", "shell.bash"]) {
            None => AlignmentVerdict::NoEvidence {
                claim: claim.clone(),
            },
            Some(o) if o.exit_code.unwrap_or(0) == 0 => AlignmentVerdict::Aligned,
            Some(o) => AlignmentVerdict::Misaligned {
                claim: claim.clone(),
                reason: format!(
                    "claim says tests pass but {} exited {} (stderr: {})",
                    o.tool,
                    o.exit_code.unwrap_or(-1),
                    truncate(&o.stderr_excerpt, 80)
                ),
            },
        },
        Claim::BuildSucceeded => {
            match latest_tool(outcomes, &["build", "cargo.build", "shell.bash"]) {
                None => AlignmentVerdict::NoEvidence {
                    claim: claim.clone(),
                },
                Some(o) if o.exit_code.unwrap_or(0) == 0 => AlignmentVerdict::Aligned,
                Some(o) => AlignmentVerdict::Misaligned {
                    claim: claim.clone(),
                    reason: format!(
                        "claim says build succeeded but {} exited {}",
                        o.tool,
                        o.exit_code.unwrap_or(-1)
                    ),
                },
            }
        }
        Claim::FileWritten { path } => {
            let hit = outcomes
                .iter()
                .find(|o| o.tool.starts_with("fs.") && o.touched_path.as_deref() == Some(path));
            match hit {
                None => AlignmentVerdict::Misaligned {
                    claim: claim.clone(),
                    reason: format!("claim says {path} was written but no fs.* tool touched it"),
                },
                Some(o) if o.exit_code.unwrap_or(0) == 0 => AlignmentVerdict::Aligned,
                Some(o) => AlignmentVerdict::Misaligned {
                    claim: claim.clone(),
                    reason: format!(
                        "claim says {path} was written but {} exited {}",
                        o.tool,
                        o.exit_code.unwrap_or(-1)
                    ),
                },
            }
        }
        Claim::TaskComplete => {
            // Soft check — flag only if any recent tool clearly failed.
            let failure = outcomes
                .iter()
                .rev()
                .take(5)
                .find(|o| o.exit_code.unwrap_or(0) != 0);
            match failure {
                None => AlignmentVerdict::Aligned,
                Some(o) => AlignmentVerdict::Misaligned {
                    claim: claim.clone(),
                    reason: format!(
                        "claim says task complete but recent {} exited {}",
                        o.tool,
                        o.exit_code.unwrap_or(-1)
                    ),
                },
            }
        }
    }
}

fn latest_tool<'a>(outcomes: &'a [ActOutcome], names: &[&str]) -> Option<&'a ActOutcome> {
    outcomes
        .iter()
        .rev()
        .find(|o| names.iter().any(|n| o.tool == *n))
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let head: String = s.chars().take(max).collect();
        format!("{head}…")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn outcome(tool: &str, exit: i32) -> ActOutcome {
        ActOutcome {
            tool: tool.into(),
            exit_code: Some(exit),
            stderr_excerpt: String::new(),
            touched_path: None,
        }
    }

    fn fs_outcome(tool: &str, path: &str, exit: i32) -> ActOutcome {
        ActOutcome {
            tool: tool.into(),
            exit_code: Some(exit),
            stderr_excerpt: String::new(),
            touched_path: Some(path.into()),
        }
    }

    #[test]
    fn parses_tests_passing_in_various_phrasing() {
        assert_eq!(parse_claims("All tests pass"), vec![Claim::TestsPassing]);
        assert_eq!(
            parse_claims("the test suite passes after my fix"),
            vec![Claim::TestsPassing]
        );
    }

    #[test]
    fn parses_file_written_with_backticks() {
        assert_eq!(
            parse_claims("I wrote `src/lib.rs` with the new module."),
            vec![Claim::FileWritten {
                path: "src/lib.rs".into(),
            }]
        );
    }

    #[test]
    fn parses_multiple_claims_in_one_message() {
        let claims = parse_claims("Build completed and all tests pass — task complete!");
        assert!(claims.contains(&Claim::BuildSucceeded));
        assert!(claims.contains(&Claim::TestsPassing));
        assert!(claims.contains(&Claim::TaskComplete));
    }

    #[test]
    fn tests_passing_aligned_when_test_tool_exit_zero() {
        let v = check(&Claim::TestsPassing, &[outcome("test", 0)]);
        assert_eq!(v, AlignmentVerdict::Aligned);
    }

    #[test]
    fn tests_passing_misaligned_when_exit_nonzero() {
        let v = check(&Claim::TestsPassing, &[outcome("test", 1)]);
        assert!(
            matches!(v, AlignmentVerdict::Misaligned { .. }),
            "got {v:?}"
        );
    }

    #[test]
    fn tests_passing_no_evidence_when_no_test_tool_in_outcomes() {
        let v = check(&Claim::TestsPassing, &[outcome("fs.read", 0)]);
        assert!(matches!(v, AlignmentVerdict::NoEvidence { .. }));
    }

    #[test]
    fn file_written_aligned_when_path_matches_and_exit_zero() {
        let v = check(
            &Claim::FileWritten {
                path: "src/lib.rs".into(),
            },
            &[fs_outcome("fs.write", "src/lib.rs", 0)],
        );
        assert_eq!(v, AlignmentVerdict::Aligned);
    }

    #[test]
    fn file_written_misaligned_when_no_fs_tool_touched_path() {
        let v = check(
            &Claim::FileWritten {
                path: "src/lib.rs".into(),
            },
            &[outcome("test", 0)],
        );
        assert!(matches!(v, AlignmentVerdict::Misaligned { .. }));
    }

    #[test]
    fn task_complete_misaligned_if_recent_tool_failed() {
        let v = check(
            &Claim::TaskComplete,
            &[outcome("test", 0), outcome("build", 1)],
        );
        assert!(matches!(v, AlignmentVerdict::Misaligned { .. }));
    }

    #[test]
    fn task_complete_aligned_when_recent_tools_all_zero() {
        let v = check(
            &Claim::TaskComplete,
            &[outcome("test", 0), outcome("build", 0)],
        );
        assert_eq!(v, AlignmentVerdict::Aligned);
    }

    #[test]
    fn unknown_phrasing_yields_no_claims() {
        assert!(parse_claims("I had a snack between turns").is_empty());
    }
}
