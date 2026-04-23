// IMPLEMENTS: D-052, D-054, D-055, D-056, D-057, D-058, D-059, D-060, D-061, D-062, D-063, D-066, D-068, D-069, D-071, D-072, D-074, D-191
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
