// IMPLEMENTS: D-268, D-269, D-270, D-271, D-272, D-273, D-274, D-275
//! Surface family — Layer 8 redefined as siblings, with the daemon /
//! RPC unchanged.
//!
//! - [`family`] — D-268: CLI / Web / Desktop / Mobile / Voice — five
//!   siblings, one daemon.
//! - [`web_subcommand`] — D-269: `harness web` localhost UI three
//!   pillars.
//! - [`semantic_rpc`] — D-270: semantic RPC aliases (non-developer
//!   vocabulary) → real RPC method names.
//! - [`recipe`] — D-271: `display:` Recipe label parsed from
//!   SKILL.md.
//! - [`belt`] — D-272: friendly MCP wrapping for mail / calendar /
//!   docs / Slack.
//! - [`narration`] — D-273: ProgressUpdate carries an "intent" line
//!   so the surface can show *why*.
//! - [`undo`] — D-274: `session.undoLastTurn()` request envelope
//!   re-using the D-091 trash.
//! - [`accessibility`] — D-275: WCAG 2.2 AA + EN 301 549 v4.1.1
//!   release blockers.

pub mod accessibility;
pub mod belt;
pub mod family;
pub mod narration;
pub mod recipe;
pub mod semantic_rpc;
pub mod undo;
pub mod web_subcommand;

pub use accessibility::{
    AccessibilityFinding, AccessibilityRule, AccessibilityVerdict, scan_for_a11y,
};
pub use belt::{BeltAdapter, FriendlyName, registered_belt};
pub use family::{SurfaceFamily, all_siblings};
pub use narration::{NarrationError, ProgressNarration, validate_narration};
pub use recipe::{Recipe, parse_skill_display};
pub use semantic_rpc::{SemanticAlias, all_aliases, resolve_alias};
pub use undo::{UndoLastTurnError, UndoLastTurnRequest, evaluate_undo};
pub use web_subcommand::{WebPillar, web_pillars};
