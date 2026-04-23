// IMPLEMENTS: D-086, D-087, D-088, D-089, D-090, D-091, D-092, D-093, D-094, D-095
pub mod edit;
pub mod glob;
pub mod grep;
pub mod read;
pub mod write;

pub use edit::EditTool;
pub use glob::GlobTool;
pub use grep::GrepTool;
pub use read::ReadTool;
pub use write::WriteTool;

use harness_tools::{Tool, ToolError};
use std::path::{Path, PathBuf};

pub(crate) fn resolve(cwd: &Path, path: &str) -> PathBuf {
    let p = Path::new(path);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        cwd.join(p)
    }
}

pub(crate) fn io_err(path: &Path, e: &std::io::Error) -> ToolError {
    ToolError::Other(format!("{}: {e}", path.display()))
}

pub fn register_all(reg: &mut harness_tools::Registry) {
    use std::sync::Arc;
    reg.register(Arc::new(ReadTool) as Arc<dyn Tool>);
    reg.register(Arc::new(WriteTool) as Arc<dyn Tool>);
    reg.register(Arc::new(EditTool) as Arc<dyn Tool>);
    reg.register(Arc::new(GlobTool) as Arc<dyn Tool>);
    reg.register(Arc::new(GrepTool) as Arc<dyn Tool>);
}
