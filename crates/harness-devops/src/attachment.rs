// IMPLEMENTS: D-252
//! Incident attachment variants — separate from the multimodal
//! `Attachment` (D-156) so SRE tools don't pull in the image/audio
//! machinery just for log handling. The blob storage layer is the
//! same; this enum is the SRE-flavoured tag.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum IncidentAttachment {
    StackTrace {
        blob_id: String,
        language: Option<String>,
    },
    LogBundle {
        blob_id: String,
        line_count: u64,
        compressed: bool,
    },
    MetricSample {
        metric: String,
        unit: String,
        samples: Vec<(i64, f64)>,
    },
    TraceSpan {
        trace_id: String,
        span_id: String,
        service: String,
        duration_ms: u64,
    },
}

impl IncidentAttachment {
    #[must_use]
    pub fn label(&self) -> &'static str {
        match self {
            Self::StackTrace { .. } => "stack_trace",
            Self::LogBundle { .. } => "log_bundle",
            Self::MetricSample { .. } => "metric_sample",
            Self::TraceSpan { .. } => "trace_span",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_variant_has_label() {
        let cases = [
            IncidentAttachment::StackTrace {
                blob_id: "b".into(),
                language: None,
            },
            IncidentAttachment::LogBundle {
                blob_id: "b".into(),
                line_count: 0,
                compressed: false,
            },
            IncidentAttachment::MetricSample {
                metric: "m".into(),
                unit: "s".into(),
                samples: vec![],
            },
            IncidentAttachment::TraceSpan {
                trace_id: "t".into(),
                span_id: "s".into(),
                service: "svc".into(),
                duration_ms: 0,
            },
        ];
        let labels: Vec<&str> = cases.iter().map(IncidentAttachment::label).collect();
        assert_eq!(
            labels,
            vec!["stack_trace", "log_bundle", "metric_sample", "trace_span"]
        );
    }

    #[test]
    fn round_trips_via_serde() {
        let a = IncidentAttachment::TraceSpan {
            trace_id: "t".into(),
            span_id: "s".into(),
            service: "svc".into(),
            duration_ms: 12,
        };
        let s = serde_json::to_string(&a).unwrap();
        assert!(s.contains("\"kind\":\"trace_span\""));
        let back: IncidentAttachment = serde_json::from_str(&s).unwrap();
        assert_eq!(back, a);
    }
}
