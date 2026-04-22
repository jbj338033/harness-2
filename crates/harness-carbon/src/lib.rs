// IMPLEMENTS: D-436, D-437, D-438, D-439, D-440
//! Per-session carbon accounting.
//!
//! - [`estimator`] — D-436: CO2e estimator from token + tool counts.
//! - [`region`] — D-437: opt-in region table (gCO2e/kWh) — ElectricityMaps style.
//! - [`shift`] — D-438: batch time-shift to lower-carbon hours.
//! - [`budget`] — D-439: carbon-axis budget cap with verdict tier.
//! - [`scope3`] — D-440: annual EU EED 2024/1364 Scope 3 cat.1 export.

pub mod budget;
pub mod estimator;
pub mod region;
pub mod scope3;
pub mod shift;

pub use budget::{CarbonBudget, CarbonBudgetVerdict, classify_carbon};
pub use estimator::{CarbonEstimate, EstimatorInputs, estimate_co2e_g};
pub use region::{REGION_INTENSITY_TABLE, RegionIntensityRow, intensity_for};
pub use scope3::{Scope3Cat1Export, build_scope3_cat1};
pub use shift::{CarbonShiftDecision, CleanWindow, schedule_shift};
