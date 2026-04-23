// IMPLEMENTS: D-243, D-247
//! Parallelism capability + WaveCoordinator plan.
//!
//! Default is `Sequential`. `Wave` requires `disjoint_check = Strict`
//! — Cognition-rule guard: parallel writes to the same file are
//! refused. The WaveCoordinator picks `max_workers` workers whose
//! `files_modified` sets are disjoint.

use crate::task::TaskRow;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DisjointCheck {
    Strict,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ParallelismCapability {
    #[default]
    Sequential,
    Wave {
        max_workers: u8,
        disjoint_check: DisjointCheck,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WaveCoordinatorPlan {
    pub correlation_id: String,
    /// Worker ids in launch order; each batch is a disjoint group.
    pub batches: Vec<Vec<String>>,
}

/// Greedy wave planner — at each step, pick the longest set of tasks
/// whose `files_modified` sets are pairwise disjoint, up to
/// `max_workers`. Tasks with unmet `depends_on` wait until the next
/// batch.
#[must_use]
pub fn plan_wave(
    correlation_id: impl Into<String>,
    tasks: &[TaskRow],
    max_workers: u8,
) -> WaveCoordinatorPlan {
    let correlation_id = correlation_id.into();
    let mut remaining: Vec<&TaskRow> = tasks.iter().collect();
    let mut completed: std::collections::BTreeSet<String> = Default::default();
    let mut batches: Vec<Vec<String>> = Vec::new();

    while !remaining.is_empty() {
        let mut batch: Vec<&TaskRow> = Vec::new();
        let mut batch_files: std::collections::BTreeSet<&String> = Default::default();
        let cap = usize::from(max_workers);
        let mut next_remaining: Vec<&TaskRow> = Vec::new();

        for task in &remaining {
            let deps_ready = task.depends_on.iter().all(|d| completed.contains(d));
            let disjoint = task.files_modified.iter().all(|f| !batch_files.contains(f));
            if batch.len() < cap && deps_ready && disjoint {
                for f in &task.files_modified {
                    batch_files.insert(f);
                }
                batch.push(task);
            } else {
                next_remaining.push(task);
            }
        }

        if batch.is_empty() {
            break;
        }
        batches.push(batch.iter().map(|t| t.id.clone()).collect());
        for t in &batch {
            completed.insert(t.id.clone());
        }
        remaining = next_remaining;
    }

    WaveCoordinatorPlan {
        correlation_id,
        batches,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task::TaskStatus;

    fn task(id: &str, deps: &[&str], files: &[&str]) -> TaskRow {
        TaskRow {
            id: id.into(),
            correlation_id: "c1".into(),
            title: id.into(),
            status: TaskStatus::Pending,
            depends_on: deps.iter().map(|s| (*s).to_string()).collect(),
            files_modified: files.iter().map(|s| (*s).to_string()).collect(),
        }
    }

    #[test]
    fn default_is_sequential() {
        assert!(matches!(
            ParallelismCapability::default(),
            ParallelismCapability::Sequential
        ));
    }

    #[test]
    fn disjoint_files_run_in_one_batch() {
        let tasks = vec![task("t1", &[], &["a.rs"]), task("t2", &[], &["b.rs"])];
        let plan = plan_wave("c1", &tasks, 4);
        assert_eq!(plan.batches.len(), 1);
        assert_eq!(plan.batches[0].len(), 2);
    }

    #[test]
    fn overlapping_files_split_across_batches() {
        let tasks = vec![task("t1", &[], &["a.rs"]), task("t2", &[], &["a.rs"])];
        let plan = plan_wave("c1", &tasks, 4);
        assert_eq!(plan.batches.len(), 2);
    }

    #[test]
    fn dependency_pushes_task_to_next_batch() {
        let tasks = vec![task("t1", &[], &["a.rs"]), task("t2", &["t1"], &["b.rs"])];
        let plan = plan_wave("c1", &tasks, 4);
        assert_eq!(plan.batches.len(), 2);
        assert_eq!(plan.batches[0], vec!["t1"]);
        assert_eq!(plan.batches[1], vec!["t2"]);
    }
}
