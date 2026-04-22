// IMPLEMENTS: D-438
//! Batch carbon-shift. Given a forecast of clean windows and an
//! upper bound on user-acceptable delay, picks the earliest window
//! whose intensity beats the now-intensity by at least `min_delta_g`.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CleanWindow {
    pub start_at_ms: i64,
    pub g_co2e_per_kwh: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CarbonShiftDecision {
    RunNow,
    DelayUntil { at_ms: i64, g_co2e_per_kwh: u32 },
}

#[must_use]
pub fn schedule_shift(
    now_ms: i64,
    now_intensity_g_per_kwh: u32,
    max_delay_ms: i64,
    min_delta_g_per_kwh: u32,
    windows: &[CleanWindow],
) -> CarbonShiftDecision {
    let cutoff = now_ms + max_delay_ms;
    let best = windows
        .iter()
        .filter(|w| w.start_at_ms >= now_ms && w.start_at_ms <= cutoff)
        .filter(|w| w.g_co2e_per_kwh + min_delta_g_per_kwh <= now_intensity_g_per_kwh)
        .min_by_key(|w| w.g_co2e_per_kwh);
    match best {
        Some(w) => CarbonShiftDecision::DelayUntil {
            at_ms: w.start_at_ms,
            g_co2e_per_kwh: w.g_co2e_per_kwh,
        },
        None => CarbonShiftDecision::RunNow,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_window_within_budget_runs_now() {
        let d = schedule_shift(
            0,
            300,
            10_000,
            50,
            &[CleanWindow {
                start_at_ms: 100_000,
                g_co2e_per_kwh: 100,
            }],
        );
        assert!(matches!(d, CarbonShiftDecision::RunNow));
    }

    #[test]
    fn picks_cleanest_inside_budget() {
        let d = schedule_shift(
            0,
            300,
            10_000,
            10,
            &[
                CleanWindow {
                    start_at_ms: 1_000,
                    g_co2e_per_kwh: 200,
                },
                CleanWindow {
                    start_at_ms: 2_000,
                    g_co2e_per_kwh: 80,
                },
            ],
        );
        match d {
            CarbonShiftDecision::DelayUntil { g_co2e_per_kwh, .. } => {
                assert_eq!(g_co2e_per_kwh, 80)
            }
            CarbonShiftDecision::RunNow => panic!("expected delay"),
        }
    }

    #[test]
    fn small_delta_skips_shift() {
        let d = schedule_shift(
            0,
            100,
            10_000,
            50,
            &[CleanWindow {
                start_at_ms: 1_000,
                g_co2e_per_kwh: 95,
            }],
        );
        assert!(matches!(d, CarbonShiftDecision::RunNow));
    }
}
