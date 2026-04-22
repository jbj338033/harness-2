// IMPLEMENTS: D-269
//! `harness web` localhost UI — three pillars: Projects, Agents,
//! Spaces. Pinned here so future surfaces can't accidentally drop one
//! of the three foundational nav entries.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WebPillar {
    Projects,
    Agents,
    Spaces,
}

#[must_use]
pub fn web_pillars() -> &'static [WebPillar] {
    use WebPillar::*;
    const ALL: &[WebPillar] = &[Projects, Agents, Spaces];
    ALL
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn three_pillars_in_order() {
        let p = web_pillars();
        assert_eq!(
            p,
            &[WebPillar::Projects, WebPillar::Agents, WebPillar::Spaces]
        );
    }
}
