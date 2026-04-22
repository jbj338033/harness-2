// IMPLEMENTS: D-437
//! Region intensity table — opt-in. Numbers are coarse 2024–25
//! averages from public ElectricityMaps / Ember snapshots; the
//! daemon refreshes through an MCP adapter when one is configured.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RegionIntensityRow {
    pub region: &'static str,
    pub g_co2e_per_kwh: u32,
}

pub const REGION_INTENSITY_TABLE: &[RegionIntensityRow] = &[
    RegionIntensityRow {
        region: "us-west-2",
        g_co2e_per_kwh: 110,
    },
    RegionIntensityRow {
        region: "us-east-1",
        g_co2e_per_kwh: 380,
    },
    RegionIntensityRow {
        region: "eu-west-1",
        g_co2e_per_kwh: 250,
    },
    RegionIntensityRow {
        region: "eu-north-1",
        g_co2e_per_kwh: 30,
    },
    RegionIntensityRow {
        region: "ap-northeast-1",
        g_co2e_per_kwh: 470,
    },
    RegionIntensityRow {
        region: "ap-northeast-2",
        g_co2e_per_kwh: 430,
    },
];

#[must_use]
pub fn intensity_for(region: &str) -> Option<u32> {
    REGION_INTENSITY_TABLE
        .iter()
        .find(|r| r.region == region)
        .map(|r| r.g_co2e_per_kwh)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_region_returns_intensity() {
        assert_eq!(intensity_for("eu-north-1"), Some(30));
    }

    #[test]
    fn unknown_region_returns_none() {
        assert!(intensity_for("mars-1").is_none());
    }

    #[test]
    fn nordic_lower_than_apac() {
        assert!(intensity_for("eu-north-1").unwrap() < intensity_for("ap-northeast-1").unwrap());
    }
}
