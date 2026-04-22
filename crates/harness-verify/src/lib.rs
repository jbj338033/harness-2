// IMPLEMENTS: D-025, D-041, D-171
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub mod capability;
pub mod regex_library;

pub use capability::{Capability, is_destructive};
pub use regex_library::{Pattern, scan};

#[derive(Debug, Error)]
pub enum VerifyError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("verifier {name} failed to start: {error}")]
    Spawn { name: String, error: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VerifyOutcome {
    Pass,
    Fail { reason: String },
    Skip { reason: String },
}

/// What the verify loop guarantees, written so a reader of `Contract` alone
/// can predict behaviour without reading every backend. Encodes D-041 #5.
#[derive(Debug, Clone, Copy)]
pub struct VerifyContract;

impl VerifyContract {
    /// Order verifiers run in. External signals come first because they are
    /// deterministic and don't burn LLM tokens (D-171a).
    pub const ORDER: &'static [&'static str] =
        &["type-check", "lint", "test", "regex", "llm-judge"];

    /// LLM judge is off unless the user opts in (D-171a).
    pub const LLM_JUDGE_OPT_IN_DEFAULT: bool = false;

    /// When the only available judge model is the same one that produced the
    /// candidate, run it under an adversarial system prompt and surface a
    /// warning to the user (D-171b).
    pub const SELF_JUDGE_REQUIRES_WARNING: bool = true;
}

#[async_trait]
pub trait Verifier: Send + Sync {
    fn name(&self) -> &'static str;

    async fn run(&self, ctx: &VerifyContext) -> Result<VerifyOutcome, VerifyError>;
}

#[derive(Debug, Clone)]
pub struct VerifyContext {
    pub cwd: std::path::PathBuf,
    pub source_text: Option<String>,
}

pub struct ExternalCommandVerifier {
    name: &'static str,
    program: String,
    args: Vec<String>,
}

impl ExternalCommandVerifier {
    #[must_use]
    pub fn new(name: &'static str, program: impl Into<String>, args: Vec<String>) -> Self {
        Self {
            name,
            program: program.into(),
            args,
        }
    }

    #[must_use]
    pub fn cargo_test() -> Self {
        Self::new("test", "cargo", vec!["test".into(), "--quiet".into()])
    }

    #[must_use]
    pub fn cargo_clippy() -> Self {
        Self::new(
            "lint",
            "cargo",
            vec!["clippy".into(), "--all-targets".into(), "--quiet".into()],
        )
    }

    #[must_use]
    pub fn cargo_check() -> Self {
        Self::new(
            "type-check",
            "cargo",
            vec!["check".into(), "--quiet".into()],
        )
    }
}

#[async_trait]
impl Verifier for ExternalCommandVerifier {
    fn name(&self) -> &'static str {
        self.name
    }

    async fn run(&self, ctx: &VerifyContext) -> Result<VerifyOutcome, VerifyError> {
        let mut command = tokio::process::Command::new(&self.program);
        command.args(&self.args).current_dir(&ctx.cwd);
        let output = command.output().await.map_err(|e| VerifyError::Spawn {
            name: self.name.into(),
            error: e.to_string(),
        })?;
        if output.status.success() {
            Ok(VerifyOutcome::Pass)
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
            Ok(VerifyOutcome::Fail { reason: stderr })
        }
    }
}

/// Scans the candidate source against the harness-owned regex library.
/// D-171c: zero supply-chain dependency on third-party rule sets.
pub struct RegexVerifier;

#[async_trait]
impl Verifier for RegexVerifier {
    fn name(&self) -> &'static str {
        "regex"
    }

    async fn run(&self, ctx: &VerifyContext) -> Result<VerifyOutcome, VerifyError> {
        let Some(text) = ctx.source_text.as_deref() else {
            return Ok(VerifyOutcome::Skip {
                reason: "no candidate source provided".into(),
            });
        };
        let hits = scan(text);
        if hits.is_empty() {
            Ok(VerifyOutcome::Pass)
        } else {
            let reasons: Vec<String> = hits
                .iter()
                .map(|h| format!("{}: {}", h.id, h.summary))
                .collect();
            Ok(VerifyOutcome::Fail {
                reason: reasons.join("; "),
            })
        }
    }
}

/// Top-level driver: runs verifiers in `Contract::ORDER` and returns the
/// first failure plus everything that ran. Skipping is non-terminal.
pub async fn run_loop(
    verifiers: &[&dyn Verifier],
    ctx: &VerifyContext,
) -> Vec<(String, VerifyOutcome)> {
    let mut report = Vec::with_capacity(verifiers.len());
    for v in verifiers {
        let name = v.name().to_string();
        let outcome = match v.run(ctx).await {
            Ok(o) => o,
            Err(e) => VerifyOutcome::Fail {
                reason: e.to_string(),
            },
        };
        let stop = matches!(outcome, VerifyOutcome::Fail { .. });
        report.push((name, outcome));
        if stop {
            break;
        }
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    struct Stub {
        name: &'static str,
        outcome: VerifyOutcome,
    }

    #[async_trait]
    impl Verifier for Stub {
        fn name(&self) -> &'static str {
            self.name
        }
        async fn run(&self, _ctx: &VerifyContext) -> Result<VerifyOutcome, VerifyError> {
            Ok(self.outcome.clone())
        }
    }

    #[tokio::test]
    async fn loop_runs_until_first_failure() {
        let v1 = Stub {
            name: "type-check",
            outcome: VerifyOutcome::Pass,
        };
        let v2 = Stub {
            name: "lint",
            outcome: VerifyOutcome::Fail {
                reason: "bad".into(),
            },
        };
        let v3 = Stub {
            name: "test",
            outcome: VerifyOutcome::Pass,
        };
        let report = run_loop(
            &[&v1, &v2, &v3],
            &VerifyContext {
                cwd: PathBuf::from("."),
                source_text: None,
            },
        )
        .await;
        assert_eq!(report.len(), 2, "must stop after first fail");
        assert_eq!(report[0].0, "type-check");
        assert_eq!(report[1].0, "lint");
    }

    #[tokio::test]
    async fn loop_skips_are_not_terminal() {
        let v1 = Stub {
            name: "regex",
            outcome: VerifyOutcome::Skip {
                reason: "no source".into(),
            },
        };
        let v2 = Stub {
            name: "test",
            outcome: VerifyOutcome::Pass,
        };
        let report = run_loop(
            &[&v1, &v2],
            &VerifyContext {
                cwd: PathBuf::from("."),
                source_text: None,
            },
        )
        .await;
        assert_eq!(report.len(), 2);
        assert!(matches!(report[0].1, VerifyOutcome::Skip { .. }));
        assert!(matches!(report[1].1, VerifyOutcome::Pass));
    }

    #[tokio::test]
    async fn regex_verifier_skips_when_source_absent() {
        let v = RegexVerifier;
        let r = v
            .run(&VerifyContext {
                cwd: PathBuf::from("."),
                source_text: None,
            })
            .await
            .unwrap();
        assert!(matches!(r, VerifyOutcome::Skip { .. }));
    }

    #[tokio::test]
    async fn regex_verifier_fails_on_known_bad_pattern() {
        let v = RegexVerifier;
        let r = v
            .run(&VerifyContext {
                cwd: PathBuf::from("."),
                source_text: Some("let x = todo!();".into()),
            })
            .await
            .unwrap();
        assert!(matches!(r, VerifyOutcome::Fail { .. }), "got {r:?}");
    }

    #[test]
    fn contract_order_matches_doc() {
        assert_eq!(
            VerifyContract::ORDER,
            &["type-check", "lint", "test", "regex", "llm-judge"]
        );
    }

    const _: () = assert!(!VerifyContract::LLM_JUDGE_OPT_IN_DEFAULT);
    const _: () = assert!(VerifyContract::SELF_JUDGE_REQUIRES_WARNING);
}
