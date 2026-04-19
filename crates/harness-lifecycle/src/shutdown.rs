use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, Default)]
pub struct Shutdown {
    token: CancellationToken,
}

impl Shutdown {
    #[must_use]
    pub fn new() -> Self {
        Self {
            token: CancellationToken::new(),
        }
    }

    pub fn trigger(&self) {
        self.token.cancel();
    }

    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.token.is_cancelled()
    }

    pub async fn cancelled(&self) {
        self.token.cancelled().await;
    }

    #[must_use]
    pub fn token(&self) -> &CancellationToken {
        &self.token
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn trigger_wakes_awaiters() {
        let s = Shutdown::new();
        let s2 = s.clone();

        let handle = tokio::spawn(async move {
            s2.cancelled().await;
            42
        });

        tokio::time::sleep(Duration::from_millis(10)).await;
        assert!(!s.is_cancelled());
        s.trigger();
        let v = handle.await.unwrap();
        assert_eq!(v, 42);
        assert!(s.is_cancelled());
    }

    #[tokio::test]
    async fn multiple_clones_all_wake() {
        let s = Shutdown::new();
        let mut handles = Vec::new();
        for _ in 0..5 {
            let c = s.clone();
            handles.push(tokio::spawn(async move { c.cancelled().await }));
        }
        s.trigger();
        for h in handles {
            h.await.unwrap();
        }
    }

    #[test]
    fn default_is_active_not_cancelled() {
        let s = Shutdown::default();
        assert!(!s.is_cancelled());
    }
}
