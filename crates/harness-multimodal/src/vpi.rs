// IMPLEMENTS: D-282
//! Visual Prompt Injection 5-layer defence. The VPI-Bench paper
//! demonstrated 100% Browser-Use compromise via on-screen text, so
//! Harness treats every screen pixel as adversarial and walks the
//! payload through five gates before any planner sees it.

use crate::Attachment;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TrustLabel {
    #[default]
    Untrusted,
    /// Screen captures get the explicit higher tier — they may contain
    /// rendered phishing text that bypasses normal "untrusted" wrapping.
    ScreenUntrusted,
    /// Caller has manually attested the image came from a trusted source
    /// (eg. a human-uploaded design mock).
    Attested,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VpiVerdict {
    /// All five layers cleared. Safe to pass to the planner.
    Pass,
    /// One layer flagged the asset; quarantine the whole session and
    /// alert the user.
    Quarantine { reason: &'static str },
}

/// D-282 layer 1: every screen capture is auto-classified as
/// `ScreenUntrusted` regardless of its incoming label.
#[must_use]
pub fn classify_screen_capture(att: &Attachment) -> TrustLabel {
    if matches!(att.kind, crate::AttachmentKind::ScreenCapture) {
        TrustLabel::ScreenUntrusted
    } else {
        att.trust
    }
}

/// D-282 layer 5: session-level quarantine. Once flipped, no further
/// attachments from this session are admitted to the planner — the
/// caller surfaces the alert and the user must explicitly resume.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionQuarantine {
    pub triggered: bool,
    pub reason: String,
    pub triggered_by_blob: Option<String>,
}

#[must_use]
pub fn quarantine_session(reason: &str, blob_id: Option<String>) -> SessionQuarantine {
    SessionQuarantine {
        triggered: true,
        reason: reason.into(),
        triggered_by_blob: blob_id,
    }
}

/// D-282 layer 2: OCR text isolation. The OCR'd string is wrapped in
/// the standard untrusted XML tag (re-using the [`harness-context`]
/// pattern) so a downstream planner can refuse it the same way it does
/// for webhook bodies.
#[must_use]
pub fn isolate_ocr_text(ocr_text: &str) -> String {
    format!("<untrusted source=\"screen-ocr\">{ocr_text}</untrusted>")
}

/// D-282 layer 3: bounding-box check. We expect the action's coordinate
/// to fall inside the captured region; out-of-bounds clicks are a
/// classic VPI tell ("click here for upgrade!" overlaid on a dialog).
#[must_use]
pub fn click_within_bounds(click_x: u32, click_y: u32, bounds: (u32, u32, u32, u32)) -> bool {
    let (x0, y0, x1, y1) = bounds;
    click_x >= x0 && click_x <= x1 && click_y >= y0 && click_y <= y1
}

/// D-282 layer 4: the planner must explicitly re-confirm an action
/// derived from screen content. We model the gate as a constant; the
/// caller wires the prompt through the existing [`harness-tools::approval`]
/// flow.
pub const REQUIRES_REAFFIRM_AFTER_VPI_LAYER: bool = true;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AttachmentKind, BlobId};

    fn att(kind: AttachmentKind, label: TrustLabel) -> Attachment {
        Attachment {
            kind,
            blob_id: BlobId("0".repeat(32)),
            mime: "image/png".into(),
            size_bytes: 0,
            caption: String::new(),
            trust: label,
        }
    }

    #[test]
    fn screen_capture_promotes_to_screen_untrusted() {
        let a = att(AttachmentKind::ScreenCapture, TrustLabel::Attested);
        assert_eq!(classify_screen_capture(&a), TrustLabel::ScreenUntrusted);
    }

    #[test]
    fn non_screen_attachment_keeps_existing_trust() {
        let a = att(AttachmentKind::Image, TrustLabel::Attested);
        assert_eq!(classify_screen_capture(&a), TrustLabel::Attested);
    }

    #[test]
    fn quarantine_records_blob_reference() {
        let q = quarantine_session("VPI layer 2 hit", Some("blob-1".into()));
        assert!(q.triggered);
        assert_eq!(q.reason, "VPI layer 2 hit");
        assert_eq!(q.triggered_by_blob.as_deref(), Some("blob-1"));
    }

    #[test]
    fn isolate_ocr_uses_screen_ocr_source() {
        let s = isolate_ocr_text("BUY NOW");
        assert!(s.starts_with("<untrusted source=\"screen-ocr\">"));
        assert!(s.contains("BUY NOW"));
        assert!(s.ends_with("</untrusted>"));
    }

    #[test]
    fn click_within_bounds_accepts_inside_and_rejects_outside() {
        assert!(click_within_bounds(50, 50, (0, 0, 100, 100)));
        assert!(!click_within_bounds(150, 50, (0, 0, 100, 100)));
        assert!(!click_within_bounds(50, 150, (0, 0, 100, 100)));
    }

    #[test]
    fn click_at_bound_edge_is_inside() {
        assert!(click_within_bounds(100, 100, (0, 0, 100, 100)));
    }
}
