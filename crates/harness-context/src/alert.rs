// IMPLEMENTS: D-176, D-177
//! Structured alert templates. D-176 (and D-177d) say `Speak(*)` alert
//! bodies must come from a fluent template catalog, never from the LLM —
//! the format-args panic risk in dynamic LLM output is too high. We hold
//! the canonical key set here and provide a tiny renderer that does
//! `{name}` substitution without touching `format!` internals.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlertKey {
    DiscrepancyAlert,
    RepeatLoopDetected,
    CostCapReached,
    InjectionInUntrusted,
    NestingTooDeep,
    SelfJudgeWarning,
}

impl AlertKey {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::DiscrepancyAlert => "speak.alert.discrepancy",
            Self::RepeatLoopDetected => "speak.alert.repeat_loop_detected",
            Self::CostCapReached => "speak.alert.cost_cap_reached",
            Self::InjectionInUntrusted => "speak.alert.injection_in_untrusted",
            Self::NestingTooDeep => "speak.alert.nesting_too_deep",
            Self::SelfJudgeWarning => "speak.alert.self_judge_warning",
        }
    }

    /// Default English template. Production should load these from the
    /// fluent (FTL) bundle by `as_str()`; this fallback keeps the system
    /// working when the bundle is absent.
    #[must_use]
    pub fn default_template(self) -> &'static str {
        match self {
            Self::DiscrepancyAlert => {
                "Speak claim {claim} contradicts the recorded result of {tool} (exit {exit_code})."
            }
            Self::RepeatLoopDetected => {
                "The same action {action_hash} ran {count} times in a row. Confirm to continue."
            }
            Self::CostCapReached => "Cost cap on {tier} reached: ${used} / ${cap}. Hard stop.",
            Self::InjectionInUntrusted => {
                "Pattern {pattern_id} found inside an untrusted block from {source}."
            }
            Self::NestingTooDeep => {
                "Untrusted block nested {depth} levels deep — limit is {max_depth}."
            }
            Self::SelfJudgeWarning => {
                "LLM self-judge active for model {model}. Verifier independence is reduced."
            }
        }
    }

    pub fn iter() -> impl Iterator<Item = Self> {
        [
            Self::DiscrepancyAlert,
            Self::RepeatLoopDetected,
            Self::CostCapReached,
            Self::InjectionInUntrusted,
            Self::NestingTooDeep,
            Self::SelfJudgeWarning,
        ]
        .into_iter()
    }
}

/// Render an alert by substituting `{name}` placeholders. Unknown
/// placeholders are left as-is — never panics, never formats arbitrary
/// LLM bytes.
#[must_use]
pub fn render(template: &str, args: &BTreeMap<String, String>) -> String {
    let mut out = String::with_capacity(template.len());
    let bytes = template.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'{'
            && let Some(rel) = bytes[i + 1..].iter().position(|b| *b == b'}')
        {
            let name = &template[i + 1..i + 1 + rel];
            if let Some(v) = args.get(name) {
                out.push_str(v);
            } else {
                out.push_str(&template[i..i + 1 + rel + 1]);
            }
            i += 1 + rel + 1;
            continue;
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_unique_and_namespaced() {
        let mut seen = std::collections::HashSet::new();
        for k in AlertKey::iter() {
            assert!(seen.insert(k.as_str()), "duplicate key {}", k.as_str());
            assert!(k.as_str().starts_with("speak.alert."));
        }
    }

    #[test]
    fn render_substitutes_known_placeholder() {
        let mut args = BTreeMap::new();
        args.insert("count".into(), "3".into());
        args.insert("action_hash".into(), "abc".into());
        let out = render(AlertKey::RepeatLoopDetected.default_template(), &args);
        assert!(out.contains("3 times"));
        assert!(out.contains("abc"));
    }

    #[test]
    fn render_leaves_unknown_placeholder_intact() {
        let args = BTreeMap::new();
        let out = render("hi {nope}!", &args);
        assert_eq!(out, "hi {nope}!");
    }

    #[test]
    fn render_handles_template_without_placeholders() {
        let out = render("plain", &BTreeMap::new());
        assert_eq!(out, "plain");
    }

    #[test]
    fn templates_have_no_format_args_panic_pattern() {
        // Ensure no `{:` style format specifiers — only `{name}` form.
        for k in AlertKey::iter() {
            let t = k.default_template();
            assert!(!t.contains("{:"), "template {t} uses format specifier");
        }
    }
}
