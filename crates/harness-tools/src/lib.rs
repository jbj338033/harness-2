// IMPLEMENTS: D-191
pub mod approval;
pub mod bash_ast;
pub mod habituation;
pub mod registry;
pub mod sandbox;
pub mod tool;

pub use approval::{ApprovalDecision, ApprovalGate, ApprovalOutcome};
pub use bash_ast::BashVerdict;
pub use habituation::{
    HABITUATION_NOTICE_KEY, HABITUATION_THRESHOLD, HabituationGuard, throttle_delay,
};
pub use registry::Registry;
pub use sandbox::{Sandbox, SandboxPolicy};
pub use tool::{ApprovalRequester, ApprovalVerdict, Tool, ToolContext, ToolError, ToolOutput};
