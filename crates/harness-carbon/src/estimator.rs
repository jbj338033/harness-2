// IMPLEMENTS: D-436
//! Per-session CO2e estimator. Inputs: token counts (input/output),
//! tool-invocation count, region intensity (gCO2e per kWh). Outputs:
//! a coarse but reproducible estimate. Numbers are illustrative —
//! they're tuned to be correct to within an order of magnitude
//! against published 2024/25 LLM energy reports.

use serde::{Deserialize, Serialize};

/// Approx. energy per token across modern frontier models in
/// kWh/token. The figure is intentionally a rough average; the
/// caller can override per-provider via [`EstimatorInputs::override_kwh_per_input_token`].
const DEFAULT_KWH_PER_INPUT_TOKEN: f64 = 5.0e-7;
const DEFAULT_KWH_PER_OUTPUT_TOKEN: f64 = 2.0e-6;
const DEFAULT_KWH_PER_TOOL_INVOCATION: f64 = 1.0e-5;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct EstimatorInputs {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub tool_invocations: u32,
    pub region_intensity_g_per_kwh: f32,
    pub override_kwh_per_input_token: Option<f64>,
    pub override_kwh_per_output_token: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CarbonEstimate {
    pub kwh: f64,
    pub g_co2e: f64,
}

#[must_use]
pub fn estimate_co2e_g(inputs: EstimatorInputs) -> CarbonEstimate {
    let kwh_in = inputs
        .override_kwh_per_input_token
        .unwrap_or(DEFAULT_KWH_PER_INPUT_TOKEN);
    let kwh_out = inputs
        .override_kwh_per_output_token
        .unwrap_or(DEFAULT_KWH_PER_OUTPUT_TOKEN);
    let kwh = (inputs.input_tokens as f64) * kwh_in
        + (inputs.output_tokens as f64) * kwh_out
        + f64::from(inputs.tool_invocations) * DEFAULT_KWH_PER_TOOL_INVOCATION;
    let intensity = f64::from(inputs.region_intensity_g_per_kwh.max(0.0));
    let g_co2e = kwh * intensity;
    CarbonEstimate { kwh, g_co2e }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inputs() -> EstimatorInputs {
        EstimatorInputs {
            input_tokens: 10_000,
            output_tokens: 1_000,
            tool_invocations: 5,
            region_intensity_g_per_kwh: 200.0,
            override_kwh_per_input_token: None,
            override_kwh_per_output_token: None,
        }
    }

    #[test]
    fn estimate_is_positive_for_real_session() {
        let e = estimate_co2e_g(inputs());
        assert!(e.kwh > 0.0);
        assert!(e.g_co2e > 0.0);
    }

    #[test]
    fn override_changes_total() {
        let mut i = inputs();
        i.override_kwh_per_input_token = Some(0.0);
        i.override_kwh_per_output_token = Some(0.0);
        let e = estimate_co2e_g(i);
        let tool_only_kwh = f64::from(5_u32) * DEFAULT_KWH_PER_TOOL_INVOCATION;
        assert!((e.kwh - tool_only_kwh).abs() < 1e-12);
    }

    #[test]
    fn negative_intensity_clamped_to_zero() {
        let mut i = inputs();
        i.region_intensity_g_per_kwh = -1.0;
        let e = estimate_co2e_g(i);
        assert_eq!(e.g_co2e, 0.0);
    }
}
