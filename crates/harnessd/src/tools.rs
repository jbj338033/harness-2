use harness_session::SessionBroadcaster;
use harness_storage::WriterHandle;
use harness_tools::Registry;
use harness_tools_agent::AgentTools;
use harness_tools_browser::BrowserTool;
#[cfg(feature = "screen-capture")]
use harness_tools_computer::{ComputerTool, NativeKeyboard, NativePointer, NativeScreen};
use harness_tools_fs::{EditTool, GlobTool, GrepTool, ReadTool, WriteTool};
use harness_tools_lsp::{LspPool, LspTool};
use harness_tools_shell::BashTool;
use harness_tools_skills::ActivateSkill;
use harness_tools_web::WebFetchTool;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::debug;
#[cfg(feature = "screen-capture")]
use tracing::warn;

pub struct RegistryInputs {
    pub writer: WriterHandle,
    pub broadcaster: Arc<SessionBroadcaster>,
    pub db_path: PathBuf,
    pub default_model: String,
    pub browser_cdp_endpoint: Option<String>,
    pub skills: Arc<std::sync::RwLock<harness_skills::Catalog>>,
}

#[must_use]
pub fn build(inputs: RegistryInputs) -> Arc<Registry> {
    let r = Registry::new();

    r.register(Arc::new(ReadTool));
    r.register(Arc::new(EditTool));
    r.register(Arc::new(WriteTool));
    r.register(Arc::new(GlobTool));
    r.register(Arc::new(GrepTool));

    r.register(Arc::new(BashTool));

    r.register(Arc::new(WebFetchTool::new()));

    let agent_tools = AgentTools {
        writer: inputs.writer.clone(),
        broadcaster: inputs.broadcaster,
        db_path: inputs.db_path.clone(),
        default_model: inputs.default_model,
    };
    agent_tools.register(&r);

    r.register(Arc::new(ActivateSkill::new(
        inputs.skills,
        inputs.writer,
        inputs.db_path,
    )));

    r.register(Arc::new(LspTool::new(LspPool::new())));

    #[cfg(feature = "screen-capture")]
    match (NativePointer::try_new(), NativeKeyboard::try_new()) {
        (Ok(pointer), Ok(keyboard)) => {
            r.register(Arc::new(ComputerTool::new(
                Arc::new(NativeScreen),
                Arc::new(pointer),
                Arc::new(keyboard),
            )));
        }
        (Err(e), _) | (_, Err(e)) => {
            warn!(error = %e, "computer tool unavailable — no display or missing permissions");
        }
    }

    if let Some(endpoint) = inputs.browser_cdp_endpoint {
        r.register(Arc::new(BrowserTool::new(endpoint)));
    }

    debug!(tool_count = r.len(), "native tool registry built");
    Arc::new(r)
}
