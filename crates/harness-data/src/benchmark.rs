// IMPLEMENTS: D-266
//! Benchmark honesty guard. Marketing copy, README claims, and skill
//! descriptions go through this filter before they ship. The CIDR
//! 2026 takeaway: parroting "GPT-X is 90% on text-to-SQL" without
//! naming Spider 2.0 (o1-preview 21.3%) is misleading.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BenchmarkClaim {
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BenchmarkVerdict {
    Clean,
    /// Triggered phrases plus the canonical sub-50% datasets the
    /// caller should disclose alongside.
    Triggered {
        matched: Vec<String>,
        must_cite: Vec<String>,
    },
}

const PUFFERY: &[&str] = &[
    ">90%",
    "90% accuracy",
    "near-perfect text-to-sql",
    "ga-ready text-to-sql",
    "production-ready text-to-sql",
    "human-level data analyst",
];

const HONEST_DATASETS: &[&str] = &["Spider 2.0", "BIRD-SQL", "DS-1000"];

#[must_use]
pub fn screen_benchmark_claim(claim: &BenchmarkClaim) -> BenchmarkVerdict {
    let lower = claim.text.to_ascii_lowercase();
    let matched: Vec<String> = PUFFERY
        .iter()
        .filter(|p| lower.contains(*p))
        .map(|p| (*p).to_string())
        .collect();
    if matched.is_empty() {
        return BenchmarkVerdict::Clean;
    }
    let cites_honest = HONEST_DATASETS.iter().any(|d| claim.text.contains(d));
    if cites_honest {
        BenchmarkVerdict::Clean
    } else {
        BenchmarkVerdict::Triggered {
            matched,
            must_cite: HONEST_DATASETS.iter().map(|s| (*s).to_string()).collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn benign_claim_clean() {
        assert_eq!(
            screen_benchmark_claim(&BenchmarkClaim {
                text: "Generates SELECT statements with explanations.".into(),
            }),
            BenchmarkVerdict::Clean
        );
    }

    #[test]
    fn puffery_without_honest_dataset_is_triggered() {
        let v = screen_benchmark_claim(&BenchmarkClaim {
            text: "Production-ready text-to-SQL.".into(),
        });
        assert!(matches!(v, BenchmarkVerdict::Triggered { .. }));
    }

    #[test]
    fn puffery_with_honest_dataset_is_clean() {
        let v = screen_benchmark_claim(&BenchmarkClaim {
            text: ">90% on a private benchmark, but only 21.3% on Spider 2.0 (o1-preview).".into(),
        });
        assert_eq!(v, BenchmarkVerdict::Clean);
    }
}
