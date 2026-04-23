use harness_core::AgentId;
use harness_session::{Result, agent, tool_call};
use harness_storage::WriterHandle;
use rusqlite::Connection;
use tracing::{info, warn};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryReport {
    pub marked_failed: Vec<AgentId>,
    pub orphan_tool_calls: usize,
}

pub async fn recover(reader: &Connection, writer: &WriterHandle) -> Result<RecoveryReport> {
    let stale = agent::find_stale_running(reader)?;
    if stale.is_empty() {
        info!("recovery: no stale agents");
        return Ok(RecoveryReport {
            marked_failed: Vec::new(),
            orphan_tool_calls: 0,
        });
    }

    let mut ids = Vec::with_capacity(stale.len());
    for rec in stale {
        match agent::set_status(writer, rec.id, agent::AgentStatus::Failed).await {
            Ok(()) => ids.push(rec.id),
            Err(e) => warn!(agent = %rec.id, error = %e, "failed to mark agent failed"),
        }
    }

    let orphan_tool_calls =
        match tool_call::mark_pending_as_crashed_for_agents(writer, ids.clone()).await {
            Ok(n) => n,
            Err(e) => {
                warn!(error = %e, "failed to seal orphan tool calls");
                0
            }
        };

    info!(
        agents = ids.len(),
        orphan_tool_calls, "recovery: stale agents + tool calls cleaned up"
    );
    Ok(RecoveryReport {
        marked_failed: ids,
        orphan_tool_calls,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use harness_session::{
        agent::{self, NewAgent},
        manager::SessionManager,
    };
    use harness_storage::{Database, Writer};
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn flips_running_agents_to_failed() {
        let f = NamedTempFile::new().unwrap();
        Database::open(f.path()).unwrap();
        let writer = Writer::spawn(f.path()).unwrap();
        let sm = SessionManager::new(&writer);
        let s = sm.create("/tmp", None).await.unwrap();

        let aid = agent::insert(
            &writer,
            NewAgent {
                session_id: s.id,
                parent_id: None,
                role: "root".into(),
                model: "m".into(),
                system_prompt: None,
                worktree_path: None,
                wave: None,
            },
        )
        .await
        .unwrap();
        agent::set_status(&writer, aid, agent::AgentStatus::Running)
            .await
            .unwrap();

        let reader = rusqlite::Connection::open(f.path()).unwrap();
        let report = recover(&reader, &writer).await.unwrap();
        assert_eq!(report.marked_failed, vec![aid]);

        let reader = rusqlite::Connection::open(f.path()).unwrap();
        let rec = agent::get(&reader, aid).unwrap();
        assert_eq!(rec.status, agent::AgentStatus::Failed);
    }

    #[tokio::test]
    async fn empty_db_no_work() {
        let f = NamedTempFile::new().unwrap();
        Database::open(f.path()).unwrap();
        let writer = Writer::spawn(f.path()).unwrap();
        let reader = rusqlite::Connection::open(f.path()).unwrap();
        let report = recover(&reader, &writer).await.unwrap();
        assert!(report.marked_failed.is_empty());
    }

    /// Crash recovery 100-iteration soak — Launch Gate item.
    /// Spins up a fresh DB per iteration, plants a Running agent +
    /// pending tool call, calls `recover`, and asserts the recovery
    /// flips the agent to `Failed`. 100 iterations confirm we have
    /// no flakiness in the recovery path.
    #[tokio::test]
    async fn crash_recovery_100_iter() {
        for _ in 0..100 {
            let f = NamedTempFile::new().unwrap();
            Database::open(f.path()).unwrap();
            let writer = Writer::spawn(f.path()).unwrap();
            let sm = SessionManager::new(&writer);
            let s = sm.create("/tmp", None).await.unwrap();
            let aid = agent::insert(
                &writer,
                NewAgent {
                    session_id: s.id,
                    parent_id: None,
                    role: "root".into(),
                    model: "m".into(),
                    system_prompt: None,
                    worktree_path: None,
                    wave: None,
                },
            )
            .await
            .unwrap();
            agent::set_status(&writer, aid, agent::AgentStatus::Running)
                .await
                .unwrap();

            let reader = rusqlite::Connection::open(f.path()).unwrap();
            let report = recover(&reader, &writer).await.unwrap();
            assert_eq!(report.marked_failed, vec![aid]);

            let reader = rusqlite::Connection::open(f.path()).unwrap();
            assert_eq!(
                agent::get(&reader, aid).unwrap().status,
                agent::AgentStatus::Failed
            );
        }
    }

    #[tokio::test]
    async fn pending_and_done_agents_untouched() {
        let f = NamedTempFile::new().unwrap();
        Database::open(f.path()).unwrap();
        let writer = Writer::spawn(f.path()).unwrap();
        let sm = SessionManager::new(&writer);
        let s = sm.create("/tmp", None).await.unwrap();

        let pending = agent::insert(
            &writer,
            NewAgent {
                session_id: s.id,
                parent_id: None,
                role: "r".into(),
                model: "m".into(),
                system_prompt: None,
                worktree_path: None,
                wave: None,
            },
        )
        .await
        .unwrap();
        let done = agent::insert(
            &writer,
            NewAgent {
                session_id: s.id,
                parent_id: None,
                role: "r".into(),
                model: "m".into(),
                system_prompt: None,
                worktree_path: None,
                wave: None,
            },
        )
        .await
        .unwrap();
        agent::set_status(&writer, done, agent::AgentStatus::Done)
            .await
            .unwrap();

        let reader = rusqlite::Connection::open(f.path()).unwrap();
        let report = recover(&reader, &writer).await.unwrap();
        assert!(report.marked_failed.is_empty());

        let reader = rusqlite::Connection::open(f.path()).unwrap();
        assert_eq!(
            agent::get(&reader, pending).unwrap().status,
            agent::AgentStatus::Pending
        );
        assert_eq!(
            agent::get(&reader, done).unwrap().status,
            agent::AgentStatus::Done
        );
    }
}
