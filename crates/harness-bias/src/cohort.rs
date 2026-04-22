// IMPLEMENTS: D-301
//! Opt-in cohort audit + NYC Local Law 144 / EU FRIA export. Local
//! Law 144 (Automated Employment Decision Tools) demands an annual
//! impact ratio; FRIA wants the same numbers in its own envelope.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CohortAuditFormat {
    NycLl144,
    EuFria,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CohortAuditExport {
    pub format: CohortAuditFormat,
    pub envelope: serde_json::Value,
}

#[must_use]
pub fn export_cohort_audit(
    format: CohortAuditFormat,
    cohort_label: &str,
    selection_rate: f64,
    reference_rate: f64,
) -> CohortAuditExport {
    let impact_ratio = if reference_rate > 0.0 {
        selection_rate / reference_rate
    } else {
        f64::NAN
    };
    let envelope = match format {
        CohortAuditFormat::NycLl144 => serde_json::json!({
            "law": "NYC Local Law 144",
            "cohort": cohort_label,
            "selection_rate": selection_rate,
            "reference_rate": reference_rate,
            "impact_ratio": impact_ratio,
            "four_fifths_pass": impact_ratio >= 0.8,
        }),
        CohortAuditFormat::EuFria => serde_json::json!({
            "framework": "EU AI Act FRIA",
            "cohort": cohort_label,
            "selection_rate": selection_rate,
            "reference_rate": reference_rate,
            "impact_ratio": impact_ratio,
        }),
    };
    CohortAuditExport { format, envelope }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nyc_envelope_marks_four_fifths_pass() {
        let e = export_cohort_audit(CohortAuditFormat::NycLl144, "race=A", 0.4, 0.5);
        assert_eq!(e.envelope["four_fifths_pass"], true);
        assert!((e.envelope["impact_ratio"].as_f64().unwrap() - 0.8).abs() < 1e-6);
    }

    #[test]
    fn nyc_envelope_marks_four_fifths_fail() {
        let e = export_cohort_audit(CohortAuditFormat::NycLl144, "race=A", 0.3, 0.5);
        assert_eq!(e.envelope["four_fifths_pass"], false);
    }

    #[test]
    fn fria_carries_framework_label() {
        let e = export_cohort_audit(CohortAuditFormat::EuFria, "age=>50", 0.5, 0.5);
        assert_eq!(e.envelope["framework"], "EU AI Act FRIA");
    }

    #[test]
    fn zero_reference_rate_yields_null_impact_ratio() {
        let e = export_cohort_audit(CohortAuditFormat::EuFria, "x", 0.5, 0.0);
        assert!(e.envelope["impact_ratio"].is_null());
    }
}
