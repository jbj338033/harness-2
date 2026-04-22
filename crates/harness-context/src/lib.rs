// IMPLEMENTS: D-152, D-163, D-176, D-177
pub mod alert;
pub mod assembly;
pub mod system_prompt;
pub mod tool_description;
pub mod untrusted;

pub use alert::{AlertKey, render as render_alert};
pub use assembly::{AssembledPrompt, AssemblyInputs, CacheBreakpoint, SkillSummary, assemble};
pub use system_prompt::{base_system_prompt, role_prompt};
pub use tool_description::format_tool_description;
pub use untrusted::{InjectionHit, MAX_NESTING_DEPTH, detect_injection, nesting_depth, wrap};
