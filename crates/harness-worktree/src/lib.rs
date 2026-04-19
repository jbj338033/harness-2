use harness_core::AgentId;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use thiserror::Error;
use tokio::process::Command;
use tracing::{debug, info, warn};

pub const MAX_AGE: Duration = Duration::from_secs(7 * 24 * 3600);

pub const WT_SUBDIR: &str = ".harness/wt";

#[derive(Debug, Error)]
pub enum WorktreeError {
    #[error("git: {0}")]
    Git(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, WorktreeError>;

#[must_use]
pub fn worktree_root(repo: &Path) -> PathBuf {
    repo.join(WT_SUBDIR)
}

#[must_use]
pub fn worktree_path(repo: &Path, agent: AgentId) -> PathBuf {
    worktree_root(repo).join(agent.as_uuid().to_string())
}

pub async fn create(repo: &Path, agent: AgentId, base_ref: Option<&str>) -> Result<PathBuf> {
    let path = worktree_path(repo, agent);
    let root = worktree_root(repo);
    tokio::fs::create_dir_all(&root).await?;

    let branch = format!("harness/wt-{}", short_id(agent));
    let base = base_ref.unwrap_or("HEAD");

    let status = Command::new("git")
        .arg("-C")
        .arg(repo)
        .arg("worktree")
        .arg("add")
        .arg("-B")
        .arg(&branch)
        .arg(&path)
        .arg(base)
        .output()
        .await;

    match status {
        Ok(out) if out.status.success() => {
            info!(path = %path.display(), "created worktree");
            Ok(path)
        }
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
            warn!(error = %stderr, "git worktree add failed; falling back to copy");
            copy_fallback(repo, &path).await?;
            Ok(path)
        }
        Err(e) => {
            warn!(error = %e, "git invocation failed; falling back to copy");
            copy_fallback(repo, &path).await?;
            Ok(path)
        }
    }
}

pub async fn reap(repo: &Path, agent: AgentId) -> Result<()> {
    let path = worktree_path(repo, agent);
    if !path.exists() {
        return Ok(());
    }

    let branch = format!("harness/wt-{}", short_id(agent));
    Command::new("git")
        .arg("-C")
        .arg(repo)
        .arg("worktree")
        .arg("remove")
        .arg("--force")
        .arg(&path)
        .output()
        .await
        .ok();

    Command::new("git")
        .arg("-C")
        .arg(repo)
        .arg("branch")
        .arg("-D")
        .arg(&branch)
        .output()
        .await
        .ok();

    if path.exists() {
        tokio::fs::remove_dir_all(&path).await?;
    }
    debug!(path = %path.display(), "reaped worktree");
    Ok(())
}

pub async fn enumerate(repo: &Path) -> Result<Vec<AgentId>> {
    let root = worktree_root(repo);
    let mut out = Vec::new();
    if !root.exists() {
        return Ok(out);
    }
    let mut entries = tokio::fs::read_dir(&root).await?;
    while let Some(entry) = entries.next_entry().await? {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if let Ok(uuid) = uuid::Uuid::parse_str(name) {
            out.push(AgentId::from(uuid));
        }
    }
    Ok(out)
}

pub async fn reconcile(repo: &Path, still_running: &[AgentId]) -> Result<Vec<AgentId>> {
    let on_disk = enumerate(repo).await?;
    let keep: std::collections::HashSet<_> = still_running.iter().copied().collect();
    let mut reaped = Vec::new();
    for aid in on_disk {
        if !keep.contains(&aid) {
            if let Err(e) = reap(repo, aid).await {
                warn!(agent = %aid, error = %e, "reconcile reap failed");
                continue;
            }
            reaped.push(aid);
        }
    }
    Ok(reaped)
}

pub async fn gc(repo: &Path, max_age: Duration) -> Result<usize> {
    let root = worktree_root(repo);
    if !root.exists() {
        return Ok(0);
    }
    let mut entries = tokio::fs::read_dir(&root).await?;
    let now = SystemTime::now();
    let mut reaped = 0usize;
    while let Some(entry) = entries.next_entry().await? {
        let meta = entry.metadata().await?;
        let age = meta
            .modified()
            .ok()
            .and_then(|m| now.duration_since(m).ok())
            .unwrap_or_default();
        if age >= max_age {
            tokio::fs::remove_dir_all(entry.path()).await.ok();
            reaped += 1;
        }
    }
    Ok(reaped)
}

async fn copy_fallback(repo: &Path, dst: &Path) -> Result<()> {
    tokio::fs::create_dir_all(dst).await?;
    let repo = repo.to_path_buf();
    let dst = dst.to_path_buf();
    tokio::task::spawn_blocking(move || -> std::io::Result<()> {
        fn cp(src: &Path, dst: &Path) -> std::io::Result<()> {
            std::fs::create_dir_all(dst)?;
            for entry in std::fs::read_dir(src)? {
                let entry = entry?;
                let path = entry.path();
                let name = entry.file_name();
                if name == ".harness" || name == ".git" {
                    continue;
                }
                let target = dst.join(&name);
                let ft = entry.file_type()?;
                if ft.is_dir() {
                    cp(&path, &target)?;
                } else if ft.is_file() {
                    std::fs::copy(&path, &target)?;
                }
            }
            Ok(())
        }
        cp(&repo, &dst)
    })
    .await
    .map_err(|e| WorktreeError::Git(e.to_string()))??;
    Ok(())
}

fn short_id(agent: AgentId) -> String {
    agent.as_uuid().to_string().chars().take(8).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command as StdCommand;
    use tempfile::TempDir;

    fn git_available() -> bool {
        StdCommand::new("git")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    fn init_repo(dir: &Path) {
        StdCommand::new("git")
            .arg("-C")
            .arg(dir)
            .arg("init")
            .arg("-q")
            .arg("-b")
            .arg("main")
            .output()
            .ok();
        StdCommand::new("git")
            .arg("-C")
            .arg(dir)
            .args(["config", "user.email", "test@example.com"])
            .output()
            .ok();
        StdCommand::new("git")
            .arg("-C")
            .arg(dir)
            .args(["config", "user.name", "test"])
            .output()
            .ok();
        std::fs::write(dir.join("README.md"), "# test").unwrap();
        StdCommand::new("git")
            .arg("-C")
            .arg(dir)
            .args(["add", "."])
            .output()
            .ok();
        StdCommand::new("git")
            .arg("-C")
            .arg(dir)
            .args(["commit", "-q", "-m", "init"])
            .output()
            .ok();
    }

    #[tokio::test]
    async fn create_and_reap_with_git() {
        if !git_available() {
            return;
        }
        let t = TempDir::new().unwrap();
        init_repo(t.path());
        let aid = AgentId::new();
        let path = create(t.path(), aid, None).await.unwrap();
        assert!(path.exists());
        assert!(path.join("README.md").exists());
        reap(t.path(), aid).await.unwrap();
        assert!(!path.exists());
    }

    #[tokio::test]
    async fn create_without_git_uses_copy_fallback() {
        let t = TempDir::new().unwrap();
        std::fs::write(t.path().join("marker"), "m").unwrap();
        std::fs::write(t.path().join("other.txt"), "o").unwrap();
        let aid = AgentId::new();
        let path = create(t.path(), aid, None).await.unwrap();
        assert!(path.join("marker").exists());
        assert!(path.join("other.txt").exists());
    }

    #[tokio::test]
    async fn enumerate_finds_existing() {
        let t = TempDir::new().unwrap();
        let aid = AgentId::new();
        create(t.path(), aid, None).await.unwrap();
        let found = enumerate(t.path()).await.unwrap();
        assert!(found.contains(&aid));
    }

    #[tokio::test]
    async fn reconcile_reaps_missing_agents() {
        let t = TempDir::new().unwrap();
        let keep_aid = AgentId::new();
        let drop_aid = AgentId::new();
        create(t.path(), keep_aid, None).await.unwrap();
        create(t.path(), drop_aid, None).await.unwrap();

        let reaped = reconcile(t.path(), &[keep_aid]).await.unwrap();
        assert_eq!(reaped, vec![drop_aid]);
        assert!(worktree_path(t.path(), keep_aid).exists());
        assert!(!worktree_path(t.path(), drop_aid).exists());
    }

    #[tokio::test]
    async fn enumerate_ignores_invalid_dirs() {
        let t = TempDir::new().unwrap();
        let root = worktree_root(t.path());
        tokio::fs::create_dir_all(&root).await.unwrap();
        tokio::fs::create_dir_all(root.join("not-a-uuid"))
            .await
            .unwrap();
        assert!(enumerate(t.path()).await.unwrap().is_empty());
    }
}
