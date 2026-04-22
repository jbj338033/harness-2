// IMPLEMENTS: D-443
//! `harness-safety` pointer + the three turn-loop hook positions.
//! DeepSeek V3.2 (~18% injection leak) and Qwen3 (~22%) make
//! Llama Guard 4 + Prompt Guard 2 the default front-and-back filter.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SafetyHookPoint {
    /// Before the model sees the system + user prompt.
    PrePrompt,
    /// Before each tool invocation.
    PreTool,
    /// After every model stream chunk (real-time leak detection).
    PostStream,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LlamaGuardSpec {
    pub crate_name: &'static str,
    pub guard_model: &'static str,
    pub prompt_guard_model: &'static str,
    pub hook_points: Vec<SafetyHookPoint>,
}

#[must_use]
pub fn all_safety_hook_points() -> &'static [SafetyHookPoint] {
    use SafetyHookPoint::*;
    const ALL: &[SafetyHookPoint] = &[PrePrompt, PreTool, PostStream];
    ALL
}

impl LlamaGuardSpec {
    #[must_use]
    pub fn default_spec() -> Self {
        Self {
            crate_name: "harness-safety",
            guard_model: "llama-guard-4",
            prompt_guard_model: "prompt-guard-2",
            hook_points: all_safety_hook_points().to_vec(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn three_hook_points_in_order() {
        assert_eq!(all_safety_hook_points().len(), 3);
        assert_eq!(all_safety_hook_points()[0], SafetyHookPoint::PrePrompt);
    }

    #[test]
    fn default_spec_uses_llama_guard_4() {
        let s = LlamaGuardSpec::default_spec();
        assert_eq!(s.guard_model, "llama-guard-4");
        assert_eq!(s.prompt_guard_model, "prompt-guard-2");
    }
}
