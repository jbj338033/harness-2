// IMPLEMENTS: D-280
//! Provider-agnostic Computer Use action vocabulary. Both
//! `computer_20251124` (Anthropic) and `computer_use_preview` (OpenAI)
//! map to the same enum so the daemon's screen-control tier never holds
//! provider-specific code.
//!
//! Resolution: Opus 4.7 ships at 2576 × 1620 vs the prior 1568 × 992;
//! [`ResolutionTier`] keeps both coordinate spaces straight so a
//! captured screenshot at one resolution and a click coordinate at the
//! other don't get crossed.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResolutionTier {
    /// Pre-Opus-4.7 baseline.
    Legacy1568x992,
    /// Opus 4.7 + most 2026 vendors.
    Modern2576x1620,
}

impl ResolutionTier {
    #[must_use]
    pub fn dims(self) -> (u32, u32) {
        match self {
            Self::Legacy1568x992 => (1568, 992),
            Self::Modern2576x1620 => (2576, 1620),
        }
    }

    /// Rescale a coordinate from `from` to `self` keeping the relative
    /// position. Used when a screenshot was captured at one resolution
    /// and the model returned a click in the other.
    #[must_use]
    pub fn rescale(self, x: u32, y: u32, from: ResolutionTier) -> (u32, u32) {
        if from == self {
            return (x, y);
        }
        let (sw, sh) = self.dims();
        let (fw, fh) = from.dims();
        let nx = u32::try_from(u64::from(x) * u64::from(sw) / u64::from(fw)).unwrap_or(u32::MAX);
        let ny = u32::try_from(u64::from(y) * u64::from(sh) / u64::from(fh)).unwrap_or(u32::MAX);
        (nx, ny)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ComputerAction {
    Screenshot,
    MouseMove { x: u32, y: u32 },
    MouseClick { x: u32, y: u32, button: MouseButton },
    MouseDoubleClick { x: u32, y: u32 },
    MouseDrag { from: (u32, u32), to: (u32, u32) },
    KeyPress { keys: Vec<String> },
    TypeText { text: String },
    Scroll { x: u32, y: u32, dx: i32, dy: i32 },
    Wait { ms: u32 },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dims_match_documented_resolutions() {
        assert_eq!(ResolutionTier::Legacy1568x992.dims(), (1568, 992));
        assert_eq!(ResolutionTier::Modern2576x1620.dims(), (2576, 1620));
    }

    #[test]
    fn rescale_identity_when_same_tier() {
        assert_eq!(
            ResolutionTier::Modern2576x1620.rescale(100, 200, ResolutionTier::Modern2576x1620),
            (100, 200)
        );
    }

    #[test]
    fn rescale_legacy_to_modern_increases_coords() {
        let (nx, ny) = ResolutionTier::Modern2576x1620.rescale(
            784,
            496, // halfway in legacy
            ResolutionTier::Legacy1568x992,
        );
        assert!(nx > 784 && nx < 1500);
        assert!(ny > 496 && ny < 900);
    }

    #[test]
    fn computer_action_serde_round_trip() {
        let a = ComputerAction::MouseClick {
            x: 10,
            y: 20,
            button: MouseButton::Right,
        };
        let s = serde_json::to_string(&a).unwrap();
        assert!(s.contains("\"kind\":\"mouse_click\""));
        let back: ComputerAction = serde_json::from_str(&s).unwrap();
        assert_eq!(back, a);
    }

    #[test]
    fn screenshot_is_a_distinct_variant() {
        let a = ComputerAction::Screenshot;
        let s = serde_json::to_string(&a).unwrap();
        assert!(s.contains("\"kind\":\"screenshot\""));
    }
}
