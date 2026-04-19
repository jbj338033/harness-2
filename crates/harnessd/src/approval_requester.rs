use async_trait::async_trait;
use harness_core::SessionId;
use harness_session::broadcast::{SessionBroadcaster, SessionEvent};
use harness_tools::{ApprovalRequester, ApprovalVerdict};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::oneshot;
use tracing::warn;

const APPROVAL_TIMEOUT: Duration = Duration::from_secs(15 * 60);

pub type PendingApprovals = Arc<Mutex<HashMap<String, oneshot::Sender<String>>>>;

#[derive(Clone)]
pub struct DaemonApprovalRequester {
    pub broadcaster: Arc<SessionBroadcaster>,
    pub pending: PendingApprovals,
}

impl std::fmt::Debug for DaemonApprovalRequester {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DaemonApprovalRequester").finish()
    }
}

#[async_trait]
impl ApprovalRequester for DaemonApprovalRequester {
    async fn request(
        &self,
        session: SessionId,
        command: String,
        pattern: String,
        reason: String,
    ) -> ApprovalVerdict {
        let request_id = uuid::Uuid::new_v4().to_string();
        let (tx, rx) = oneshot::channel::<String>();

        if let Ok(mut map) = self.pending.lock() {
            map.insert(request_id.clone(), tx);
        } else {
            warn!("approval pending map poisoned; denying");
            return ApprovalVerdict::Denied;
        }

        let receivers = self.broadcaster.publish(
            session,
            SessionEvent::ApprovalRequest {
                request_id: request_id.clone(),
                command: command.clone(),
                pattern,
                reason,
            },
        );
        if receivers == 0 {
            if let Ok(mut map) = self.pending.lock() {
                map.remove(&request_id);
            }
            warn!(%command, "no client subscribed to approval prompt; denying");
            return ApprovalVerdict::Denied;
        }

        match tokio::time::timeout(APPROVAL_TIMEOUT, rx).await {
            Ok(Ok(decision)) => match decision.as_str() {
                "allow" | "allow_session" | "allow_global" => ApprovalVerdict::Allowed,
                _ => ApprovalVerdict::Denied,
            },
            Ok(Err(_)) => ApprovalVerdict::Denied,
            Err(_) => {
                if let Ok(mut map) = self.pending.lock() {
                    map.remove(&request_id);
                }
                warn!(%command, "approval timed out; denying");
                ApprovalVerdict::Denied
            }
        }
    }
}
