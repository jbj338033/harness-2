// IMPLEMENTS: D-391
//! Geofence + motion envelope. Every commanded pose flows through
//! `clamp_pose`. Out-of-envelope poses are *refused*, not silently
//! clamped — silently clamping would mask a planner bug and surface
//! it as a slow drift. The peak-velocity cap is similarly hard-fail.

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Pose {
    pub x_m: f64,
    pub y_m: f64,
    pub z_m: f64,
    pub speed_mps: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MotionEnvelope {
    pub min: (f64, f64, f64),
    pub max: (f64, f64, f64),
    pub max_speed_mps: f64,
}

#[derive(Debug, Error, PartialEq)]
pub enum GeofenceError {
    #[error("pose outside geofence: ({x}, {y}, {z}) not in envelope")]
    OutOfBox { x: f64, y: f64, z: f64 },
    #[error("commanded speed {commanded} exceeds cap {cap} m/s")]
    SpeedExceeded { commanded: f64, cap: f64 },
    #[error("non-finite pose component (NaN / inf)")]
    NonFinite,
}

pub fn clamp_pose(pose: Pose, env: MotionEnvelope) -> Result<Pose, GeofenceError> {
    if !pose.x_m.is_finite()
        || !pose.y_m.is_finite()
        || !pose.z_m.is_finite()
        || !pose.speed_mps.is_finite()
    {
        return Err(GeofenceError::NonFinite);
    }
    if pose.x_m < env.min.0
        || pose.x_m > env.max.0
        || pose.y_m < env.min.1
        || pose.y_m > env.max.1
        || pose.z_m < env.min.2
        || pose.z_m > env.max.2
    {
        return Err(GeofenceError::OutOfBox {
            x: pose.x_m,
            y: pose.y_m,
            z: pose.z_m,
        });
    }
    if pose.speed_mps > env.max_speed_mps {
        return Err(GeofenceError::SpeedExceeded {
            commanded: pose.speed_mps,
            cap: env.max_speed_mps,
        });
    }
    Ok(pose)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env() -> MotionEnvelope {
        MotionEnvelope {
            min: (-1.0, -1.0, 0.0),
            max: (1.0, 1.0, 1.5),
            max_speed_mps: 0.5,
        }
    }

    #[test]
    fn pose_inside_envelope_passes() {
        let p = Pose {
            x_m: 0.0,
            y_m: 0.0,
            z_m: 0.5,
            speed_mps: 0.2,
        };
        assert_eq!(clamp_pose(p, env()).unwrap(), p);
    }

    #[test]
    fn out_of_box_refused() {
        let p = Pose {
            x_m: 5.0,
            y_m: 0.0,
            z_m: 0.0,
            speed_mps: 0.1,
        };
        assert!(matches!(
            clamp_pose(p, env()),
            Err(GeofenceError::OutOfBox { .. })
        ));
    }

    #[test]
    fn speed_exceeded_refused() {
        let p = Pose {
            x_m: 0.0,
            y_m: 0.0,
            z_m: 0.0,
            speed_mps: 1.0,
        };
        assert!(matches!(
            clamp_pose(p, env()),
            Err(GeofenceError::SpeedExceeded { .. })
        ));
    }

    #[test]
    fn nan_pose_refused() {
        let p = Pose {
            x_m: f64::NAN,
            y_m: 0.0,
            z_m: 0.0,
            speed_mps: 0.1,
        };
        assert!(matches!(
            clamp_pose(p, env()),
            Err(GeofenceError::NonFinite)
        ));
    }
}
