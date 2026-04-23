// IMPLEMENTS: D-186
//! Agent-written memory bias audit. Periodically samples the
//! long-term memory entries and counts mentions across the eight
//! protected axes. A drift score (max axis share - min axis share)
//! flags imbalance.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BiasAxis {
    Race,
    Gender,
    Age,
    Disability,
    Religion,
    Nationality,
    Sexuality,
    SocioEconomic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BiasAxisCount {
    pub axis: BiasAxis,
    pub mentions: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemoryBiasAuditReport {
    pub axis_counts: Vec<BiasAxisCount>,
    pub total: u32,
    pub drift_score: f32,
}

#[must_use]
pub fn audit_memory_bias(counts: Vec<BiasAxisCount>) -> MemoryBiasAuditReport {
    let total: u32 = counts.iter().map(|c| c.mentions).sum();
    let drift_score = if total == 0 {
        0.0
    } else {
        let total_f32 = total as f32;
        let max = counts.iter().map(|c| c.mentions).max().unwrap_or(0);
        let min = counts.iter().map(|c| c.mentions).min().unwrap_or(0);
        (max - min) as f32 / total_f32
    };
    MemoryBiasAuditReport {
        axis_counts: counts,
        total,
        drift_score,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_report_drift_zero() {
        let r = audit_memory_bias(vec![]);
        assert_eq!(r.drift_score, 0.0);
        assert_eq!(r.total, 0);
    }

    #[test]
    fn balanced_axes_low_drift() {
        let r = audit_memory_bias(vec![
            BiasAxisCount {
                axis: BiasAxis::Gender,
                mentions: 10,
            },
            BiasAxisCount {
                axis: BiasAxis::Race,
                mentions: 11,
            },
        ]);
        assert!(r.drift_score < 0.1);
    }

    #[test]
    fn skewed_axes_high_drift() {
        let r = audit_memory_bias(vec![
            BiasAxisCount {
                axis: BiasAxis::Gender,
                mentions: 100,
            },
            BiasAxisCount {
                axis: BiasAxis::Race,
                mentions: 0,
            },
        ]);
        assert!(r.drift_score >= 0.99);
    }
}
