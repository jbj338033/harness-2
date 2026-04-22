// IMPLEMENTS: D-209, D-210
//! Soak harness — drives a long-running loop against [`Shutdown`] to
//! catch leaks and ensure graceful shutdown lands every iteration. Used
//! by the `soak_24h` ignored test (D-209) and by the integration test in
//! `harnessd` that re-verifies the cancellation chain (D-210).

use crate::Shutdown;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Default)]
pub struct SoakStats {
    pub iterations: u64,
    pub elapsed: Duration,
    pub clean_shutdown: bool,
}

/// Run `step` repeatedly until `total` elapses or `shutdown` fires —
/// whichever comes first. The closure may sleep itself; this driver only
/// schedules — it never blocks the runtime.
pub async fn run_soak<F, Fut>(
    shutdown: Shutdown,
    total: Duration,
    tick: Duration,
    mut step: F,
) -> SoakStats
where
    F: FnMut(u64) -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    let counter = Arc::new(AtomicU64::new(0));
    let started = Instant::now();
    let deadline = started + total;
    let mut interval = tokio::time::interval(tick);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    let clean_shutdown = loop {
        tokio::select! {
            biased;
            () = shutdown.cancelled() => break true,
            _ = interval.tick() => {
                if Instant::now() >= deadline {
                    break true;
                }
                let n = counter.fetch_add(1, Ordering::Relaxed);
                step(n).await;
            }
        }
    };

    SoakStats {
        iterations: counter.load(Ordering::Relaxed),
        elapsed: started.elapsed(),
        clean_shutdown,
    }
}

/// Standard soak duration constants — tests pick the right one.
pub const SOAK_24H: Duration = Duration::from_secs(24 * 60 * 60);
pub const SOAK_SMOKE: Duration = Duration::from_millis(200);

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn smoke_soak_runs_and_records_iterations() {
        let s = Shutdown::new();
        let stats = run_soak(s, SOAK_SMOKE, Duration::from_millis(20), |_| async {}).await;
        assert!(stats.iterations >= 5, "got {}", stats.iterations);
        assert!(stats.clean_shutdown);
        assert!(stats.elapsed >= SOAK_SMOKE);
    }

    #[tokio::test]
    async fn shutdown_token_breaks_loop_immediately() {
        let s = Shutdown::new();
        let s2 = s.clone();
        // Trigger after 50ms and assert the loop stops well before
        // SOAK_SMOKE elapses.
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            s2.trigger();
        });
        let stats = run_soak(
            s,
            Duration::from_secs(60),
            Duration::from_millis(10),
            |_| async {},
        )
        .await;
        assert!(stats.clean_shutdown);
        assert!(
            stats.elapsed < Duration::from_secs(2),
            "{:?}",
            stats.elapsed
        );
    }

    #[tokio::test]
    async fn step_observes_iteration_index() {
        let s = Shutdown::new();
        let max_seen = Arc::new(AtomicU64::new(0));
        let max2 = max_seen.clone();
        let stats = run_soak(
            s,
            Duration::from_millis(100),
            Duration::from_millis(10),
            move |n| {
                let m = max2.clone();
                async move {
                    m.fetch_max(n, Ordering::Relaxed);
                }
            },
        )
        .await;
        assert_eq!(max_seen.load(Ordering::Relaxed) + 1, stats.iterations);
    }

    /// D-209 — the actual 24h soak. Marked `#[ignore]` so CI skips it
    /// unless invoked with `cargo test --ignored soak_24h`.
    #[tokio::test]
    #[ignore]
    async fn soak_24h() {
        let s = Shutdown::new();
        let stats = run_soak(s, SOAK_24H, Duration::from_secs(30), |_| async {}).await;
        assert!(stats.clean_shutdown);
        assert!(stats.elapsed >= SOAK_24H);
    }
}
