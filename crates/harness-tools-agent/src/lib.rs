mod cancel;
mod spawn;
mod wait;

pub use cancel::CancelTool;
pub use spawn::SpawnTool;
pub use wait::WaitTool;

use harness_session::SessionBroadcaster;
use harness_storage::WriterHandle;
use std::sync::Arc;

#[derive(Clone)]
pub struct AgentTools {
    pub writer: WriterHandle,
    pub broadcaster: Arc<SessionBroadcaster>,
    pub db_path: std::path::PathBuf,
    pub default_model: String,
}

impl AgentTools {
    pub fn register(self, registry: &harness_tools::Registry) {
        let arc = Arc::new(self);
        registry.register(Arc::new(SpawnTool(arc.clone())));
        registry.register(Arc::new(WaitTool(arc.clone())));
        registry.register(Arc::new(CancelTool(arc)));
    }
}
