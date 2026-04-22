// IMPLEMENTS: D-337
//! Reversal workflow generator. EU PLD 2026-12 (strict liability for
//! defective digital products) makes the operator liable for any
//! destructive action whose reversal path isn't documented up-front. We
//! synthesise a step-by-step playbook from the action's effect category
//! so the daemon can attach it to every Speak(Destructive) event.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionEffect {
    FileDeleted,
    FileOverwritten,
    DatabaseRowDropped,
    GitForcePushed,
    EmailSent,
    SchedulerJobScheduled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReversalStep {
    pub order: u32,
    pub instruction: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReversalWorkflow {
    pub effect: ActionEffect,
    pub target: String,
    pub steps: Vec<ReversalStep>,
}

/// Build a reversal workflow for the given destructive effect on
/// `target` (a path, table name, ref, recipient, etc.).
#[must_use]
pub fn build_reversal(effect: ActionEffect, target: impl Into<String>) -> ReversalWorkflow {
    let steps: &[&'static str] = match effect {
        ActionEffect::FileDeleted => &[
            "Restore from `.harness/backups/<sha>` if the file was tracked",
            "Run `git restore <path>` if the deletion happened inside the repo",
            "If neither, recover from your filesystem snapshot",
        ],
        ActionEffect::FileOverwritten => &[
            "Open `.git/lfs/objects/<sha>` if LFS-tracked",
            "Otherwise `git checkout HEAD -- <path>` to revert",
            "Verify the file matches the pre-overwrite hash",
        ],
        ActionEffect::DatabaseRowDropped => &[
            "Stop further writes to the table",
            "Restore from the most recent point-in-time backup",
            "Replay events from `harness.db.backup` since the backup epoch",
        ],
        ActionEffect::GitForcePushed => &[
            "Locate the prior tip via `git reflog show <ref>`",
            "Recover with `git push --force-with-lease <remote> <ref>:<old-sha>`",
            "Notify collaborators their local clones now have orphan commits",
        ],
        ActionEffect::EmailSent => &[
            "Issue an immediate apology / correction email referencing the message id",
            "Contact the recipient out-of-band (phone / chat) if the action was high-stakes",
            "Disable the affected mail credential if the send was unintended",
        ],
        ActionEffect::SchedulerJobScheduled => &[
            "Cancel the job via `harness scheduler cancel <id>`",
            "Verify no other scheduler tier (cron, launchd, systemd) was also primed",
            "If the job already fired, follow the underlying reversal for that effect",
        ],
    };
    ReversalWorkflow {
        effect,
        target: target.into(),
        steps: steps
            .iter()
            .enumerate()
            .map(|(i, s)| ReversalStep {
                order: u32::try_from(i + 1).unwrap_or(u32::MAX),
                instruction: (*s).to_string(),
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_deleted_workflow_includes_backup_restore() {
        let w = build_reversal(ActionEffect::FileDeleted, "src/lib.rs");
        assert!(w.steps.iter().any(|s| s.instruction.contains("backups")));
        assert_eq!(w.steps[0].order, 1);
        assert_eq!(w.target, "src/lib.rs");
    }

    #[test]
    fn force_push_workflow_mentions_reflog() {
        let w = build_reversal(ActionEffect::GitForcePushed, "main");
        assert!(w.steps.iter().any(|s| s.instruction.contains("reflog")));
    }

    #[test]
    fn email_workflow_advises_apology() {
        let w = build_reversal(ActionEffect::EmailSent, "boss@example.com");
        assert!(w.steps.iter().any(|s| s.instruction.contains("apology")));
    }

    #[test]
    fn every_effect_has_at_least_two_steps() {
        for effect in [
            ActionEffect::FileDeleted,
            ActionEffect::FileOverwritten,
            ActionEffect::DatabaseRowDropped,
            ActionEffect::GitForcePushed,
            ActionEffect::EmailSent,
            ActionEffect::SchedulerJobScheduled,
        ] {
            let w = build_reversal(effect, "target");
            assert!(w.steps.len() >= 2, "{effect:?}");
        }
    }

    #[test]
    fn step_orders_start_at_one_and_are_contiguous() {
        let w = build_reversal(ActionEffect::DatabaseRowDropped, "users");
        for (i, s) in w.steps.iter().enumerate() {
            assert_eq!(s.order, u32::try_from(i + 1).unwrap());
        }
    }
}
