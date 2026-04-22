// IMPLEMENTS: D-268
//! Surface family — Layer 8 redefined. The daemon and RPC stay
//! unchanged; only the front surface varies.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SurfaceFamily {
    Cli,
    Web,
    Desktop,
    Mobile,
    Voice,
}

#[must_use]
pub fn all_siblings() -> &'static [SurfaceFamily] {
    use SurfaceFamily::*;
    const ALL: &[SurfaceFamily] = &[Cli, Web, Desktop, Mobile, Voice];
    ALL
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn five_siblings() {
        assert_eq!(all_siblings().len(), 5);
    }

    #[test]
    fn includes_voice() {
        assert!(all_siblings().contains(&SurfaceFamily::Voice));
    }
}
