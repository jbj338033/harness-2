// IMPLEMENTS: D-390, D-391, D-392, D-393, D-394, D-395, D-396
//! Robotics mode (axis off by default).
//!
//! - [`tier`] — D-390: `PhysicalTier` sits above `Destructive` because
//!   the irreversibility axis collapses to zero (you can't undo a
//!   crushed finger).
//! - [`envelope`] — D-391: geofence + motion envelope. The
//!   commanded pose is clamped to a 3D AABB plus a peak-velocity cap.
//! - [`bridge`] — D-392: ROS 2 MCP bridge scope. We do not write
//!   native ROS code; an MCP adapter expresses topics/services as
//!   tools.
//! - [`perception`] — D-393: dual-check perception with a confidence
//!   gate. UC Santa Cruz showed environment-IPI hits 64% ASR against
//!   single-stream perception.
//! - [`estop`] — D-394: Emergency Stop in three independent tiers
//!   (SW, robot, HW). SW is fallback, HW is final.
//! - [`sim_to_real`] — D-395: sim-to-real gate + deployment tag.
//! - [`teleop`] — D-396: teleop principal + third-party consent.

pub mod bridge;
pub mod envelope;
pub mod estop;
pub mod perception;
pub mod sim_to_real;
pub mod teleop;
pub mod tier;

pub use bridge::{Ros2BridgeKind, Ros2McpScope};
pub use envelope::{GeofenceError, MotionEnvelope, Pose, clamp_pose};
pub use estop::{EmergencyStop, EstopOutcome, EstopTier, request_estop};
pub use perception::{PerceptionInput, PerceptionVerdict, PerceptionViolation, classify};
pub use sim_to_real::{DeploymentTag, SimToRealError, gate_sim_to_real};
pub use teleop::{TeleopConsentError, TeleopRequest, evaluate_teleop_consent};
pub use tier::{PhysicalTier, dominates_destructive};
