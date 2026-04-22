// IMPLEMENTS: D-164
//! Same-action detector. D-164 supersedes D-151 with three rules:
//!
//! 1. Normalize the input before hashing (whitespace collapse + lowercase
//!    + JSON key sort) so superficial diffs don't slip past the detector.
//! 2. Sliding window of the most recent 10 turns within a session.
//! 3. After the user marks an alert "legit repeat", suppress further
//!    alerts on the same hash for the next 5 turns.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, VecDeque};

pub const DEFAULT_WINDOW_TURNS: usize = 10;
pub const DEFAULT_REPEAT_THRESHOLD: usize = 3;
pub const DEFAULT_SUPPRESS_TURNS: usize = 5;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DetectorVerdict {
    Ok,
    Repeated {
        action_hash: String,
        count: usize,
    },
    Suppressed {
        action_hash: String,
        remaining_turns: usize,
    },
}

#[derive(Debug, Clone)]
pub struct SameActionDetector {
    window: VecDeque<String>,
    capacity: usize,
    threshold: usize,
    /// hash → remaining suppressed turns
    suppressed: BTreeMap<String, usize>,
    suppress_window: usize,
}

impl Default for SameActionDetector {
    fn default() -> Self {
        Self::with_window(
            DEFAULT_WINDOW_TURNS,
            DEFAULT_REPEAT_THRESHOLD,
            DEFAULT_SUPPRESS_TURNS,
        )
    }
}

impl SameActionDetector {
    #[must_use]
    pub fn with_window(capacity: usize, threshold: usize, suppress_window: usize) -> Self {
        Self {
            window: VecDeque::with_capacity(capacity.max(1)),
            capacity: capacity.max(1),
            threshold: threshold.max(2),
            suppressed: BTreeMap::new(),
            suppress_window: suppress_window.max(1),
        }
    }

    /// Observe a (tool, input) pair. The input is normalized per D-164a
    /// before hashing.
    pub fn observe(&mut self, tool: &str, input: &Value) -> DetectorVerdict {
        let hash = hash_action(tool, input);
        if let Some(remaining) = self.suppressed.get(&hash).copied() {
            self.push_window(hash.clone());
            self.tick_suppress();
            return DetectorVerdict::Suppressed {
                action_hash: hash,
                remaining_turns: remaining,
            };
        }
        self.tick_suppress();
        self.push_window(hash.clone());
        let count = self.window.iter().filter(|x| **x == hash).count();
        if count >= self.threshold {
            DetectorVerdict::Repeated {
                action_hash: hash,
                count,
            }
        } else {
            DetectorVerdict::Ok
        }
    }

    /// User responded "this loop is legitimate" — suppress the same hash
    /// for the configured suppress window.
    pub fn mark_legit(&mut self, action_hash: impl Into<String>) {
        self.suppressed
            .insert(action_hash.into(), self.suppress_window);
    }

    /// Wipe history (eg. when a session resets after a hard cost stop).
    pub fn reset(&mut self) {
        self.window.clear();
        self.suppressed.clear();
    }

    fn push_window(&mut self, hash: String) {
        if self.window.len() == self.capacity {
            self.window.pop_front();
        }
        self.window.push_back(hash);
    }

    fn tick_suppress(&mut self) {
        let mut expired: Vec<String> = Vec::new();
        for (k, v) in self.suppressed.iter_mut() {
            *v = v.saturating_sub(1);
            if *v == 0 {
                expired.push(k.clone());
            }
        }
        for k in expired {
            self.suppressed.remove(&k);
        }
    }
}

/// D-164a: canonical hash of `(tool, normalize(input))`. Whitespace inside
/// strings is collapsed, ASCII case-folded, and JSON object keys are
/// sorted so semantically-identical inputs hash the same.
#[must_use]
pub fn hash_action(tool: &str, input: &Value) -> String {
    let canonical = normalize_value(input);
    let serialized = serde_json::to_vec(&canonical).unwrap_or_default();
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"harness/same-action/v1\n");
    hasher.update(tool.as_bytes());
    hasher.update(b"\n");
    hasher.update(&serialized);
    hasher.finalize().to_hex().chars().take(32).collect()
}

fn normalize_value(value: &Value) -> Value {
    match value {
        Value::Null | Value::Bool(_) | Value::Number(_) => value.clone(),
        Value::String(s) => Value::String(normalize_string(s)),
        Value::Array(arr) => Value::Array(arr.iter().map(normalize_value).collect()),
        Value::Object(map) => {
            let mut sorted: BTreeMap<String, Value> = BTreeMap::new();
            for (k, v) in map {
                sorted.insert(k.clone(), normalize_value(v));
            }
            let mut out = serde_json::Map::with_capacity(sorted.len());
            for (k, v) in sorted {
                out.insert(k, v);
            }
            Value::Object(out)
        }
    }
}

fn normalize_string(s: &str) -> String {
    s.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn whitespace_only_diff_hashes_the_same() {
        let a = hash_action("fs.read", &json!({"path": "src/lib.rs"}));
        let b = hash_action("fs.read", &json!({"path": "  src/lib.rs  "}));
        assert_eq!(a, b);
    }

    #[test]
    fn case_diff_hashes_the_same() {
        let a = hash_action("fs.read", &json!({"path": "Hello.RS"}));
        let b = hash_action("fs.read", &json!({"path": "hello.rs"}));
        assert_eq!(a, b);
    }

    #[test]
    fn key_order_does_not_change_hash() {
        let a = hash_action("fs.read", &json!({"path": "a", "limit": 10}));
        let b = hash_action("fs.read", &json!({"limit": 10, "path": "a"}));
        assert_eq!(a, b);
    }

    #[test]
    fn different_tool_names_hash_differently() {
        let a = hash_action("fs.read", &json!({}));
        let b = hash_action("fs.write", &json!({}));
        assert_ne!(a, b);
    }

    #[test]
    fn three_observes_in_window_trigger_repeat() {
        let mut d = SameActionDetector::default();
        let input = json!({"path": "x"});
        assert_eq!(d.observe("t", &input), DetectorVerdict::Ok);
        assert_eq!(d.observe("t", &input), DetectorVerdict::Ok);
        let v = d.observe("t", &input);
        assert!(
            matches!(v, DetectorVerdict::Repeated { count: 3, .. }),
            "got {v:?}"
        );
    }

    #[test]
    fn legit_marker_suppresses_then_decays() {
        let mut d = SameActionDetector::default();
        let input = json!({"path": "x"});
        d.observe("t", &input);
        d.observe("t", &input);
        let alert = d.observe("t", &input);
        let DetectorVerdict::Repeated { action_hash, .. } = alert else {
            panic!("expected repeat");
        };
        d.mark_legit(action_hash.clone());
        // Next 5 observes report Suppressed (with decreasing remaining).
        for expected_remaining in (1..=5).rev() {
            let v = d.observe("t", &input);
            match v {
                DetectorVerdict::Suppressed {
                    action_hash: got,
                    remaining_turns,
                } => {
                    assert_eq!(got, action_hash);
                    assert_eq!(remaining_turns, expected_remaining);
                }
                other => {
                    panic!("expected Suppressed remaining={expected_remaining}, got {other:?}")
                }
            }
        }
    }

    #[test]
    fn old_actions_age_out_of_window() {
        let mut d = SameActionDetector::with_window(3, 3, 5);
        let target = json!({"k": "target"});
        d.observe("t", &target);
        for _ in 0..3 {
            d.observe("t", &json!({"k": "filler"}));
        }
        // Window is now [filler, filler, filler] — target observed twice
        // more should still be Ok.
        d.observe("t", &target);
        assert_eq!(d.observe("t", &target), DetectorVerdict::Ok);
    }

    #[test]
    fn reset_clears_window_and_suppression() {
        let mut d = SameActionDetector::default();
        d.observe("t", &json!({}));
        d.mark_legit(hash_action("t", &json!({})));
        d.reset();
        assert_eq!(d.observe("t", &json!({})), DetectorVerdict::Ok);
    }
}
