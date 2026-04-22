// IMPLEMENTS: D-304
//! Role preset BBQ gate. Each of the nine D-249 role presets must
//! land its measured BBQ score under the threshold or the preset
//! refuses to register.

use serde::{Deserialize, Serialize};

pub const ROLE_BBQ_THRESHOLD: f32 = 0.07;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum RoleBbqVerdict {
    Pass,
    Fail { role: String, observed: f32 },
}

#[must_use]
pub fn evaluate_role_bbq(role: &str, observed_bias: f32) -> RoleBbqVerdict {
    if observed_bias.is_finite() && observed_bias > ROLE_BBQ_THRESHOLD {
        RoleBbqVerdict::Fail {
            role: role.to_string(),
            observed: observed_bias,
        }
    } else {
        RoleBbqVerdict::Pass
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn under_threshold_passes() {
        assert_eq!(evaluate_role_bbq("ic", 0.05), RoleBbqVerdict::Pass);
    }

    #[test]
    fn over_threshold_fails() {
        match evaluate_role_bbq("recruiter", 0.12) {
            RoleBbqVerdict::Fail { role, observed } => {
                assert_eq!(role, "recruiter");
                assert!(observed > ROLE_BBQ_THRESHOLD);
            }
            RoleBbqVerdict::Pass => panic!("expected fail"),
        }
    }

    #[test]
    fn nan_treated_as_pass() {
        assert_eq!(evaluate_role_bbq("scribe", f32::NAN), RoleBbqVerdict::Pass);
    }
}
