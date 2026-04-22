// IMPLEMENTS: D-283
//! Multimodal full-text-search index. Each [`Attachment`] becomes one
//! row keyed by [`BlobId`]; the searchable corpus is the concat of the
//! caption (provided synchronously) plus async-extracted OCR / STT /
//! caption text. `D-121` already gave us a trigram FTS; this module is
//! the multimodal counterpart that feeds the same FTS table.

use crate::{Attachment, BlobId};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AttachmentIndex {
    rows: BTreeMap<BlobId, IndexRow>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct IndexRow {
    pub kind: String,
    pub mime: String,
    pub caption: String,
    pub ocr_text: String,
    pub stt_text: String,
}

impl IndexRow {
    /// Concatenate every searchable surface — used as the FTS body.
    #[must_use]
    pub fn corpus(&self) -> String {
        let parts = [
            self.caption.as_str(),
            self.ocr_text.as_str(),
            self.stt_text.as_str(),
        ];
        parts
            .iter()
            .filter(|p| !p.is_empty())
            .copied()
            .collect::<Vec<_>>()
            .join("\n")
    }
}

impl AttachmentIndex {
    pub fn upsert(&mut self, att: &Attachment) -> &IndexRow {
        let row = self.rows.entry(att.blob_id.clone()).or_default();
        row.kind = format!("{:?}", att.kind).to_ascii_lowercase();
        row.mime = att.mime.clone();
        row.caption = att.caption.clone();
        row
    }

    pub fn set_ocr(&mut self, blob: &BlobId, text: impl Into<String>) {
        if let Some(row) = self.rows.get_mut(blob) {
            row.ocr_text = text.into();
        }
    }

    pub fn set_stt(&mut self, blob: &BlobId, text: impl Into<String>) {
        if let Some(row) = self.rows.get_mut(blob) {
            row.stt_text = text.into();
        }
    }

    #[must_use]
    pub fn get(&self, blob: &BlobId) -> Option<&IndexRow> {
        self.rows.get(blob)
    }

    /// Pure-Rust substring search across the concatenated corpus —
    /// stand-in for the SQLite trigram FTS until the full pipeline
    /// lands. Case-insensitive.
    #[must_use]
    pub fn search(&self, query: &str) -> Vec<&BlobId> {
        let q = query.to_ascii_lowercase();
        let mut hits: Vec<&BlobId> = self
            .rows
            .iter()
            .filter(|(_, r)| r.corpus().to_ascii_lowercase().contains(&q))
            .map(|(id, _)| id)
            .collect();
        hits.sort();
        hits
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AttachmentKind, BlobId, vpi::TrustLabel};

    fn att(blob: &str, caption: &str) -> Attachment {
        Attachment {
            kind: AttachmentKind::Image,
            blob_id: BlobId(blob.into()),
            mime: "image/png".into(),
            size_bytes: 0,
            caption: caption.into(),
            trust: TrustLabel::default(),
        }
    }

    #[test]
    fn upsert_creates_row_with_caption() {
        let mut idx = AttachmentIndex::default();
        idx.upsert(&att("a", "blue car"));
        let row = idx.get(&BlobId("a".into())).unwrap();
        assert_eq!(row.caption, "blue car");
        assert_eq!(row.kind, "image");
    }

    #[test]
    fn ocr_and_stt_are_settable_post_upsert() {
        let mut idx = AttachmentIndex::default();
        idx.upsert(&att("b", ""));
        idx.set_ocr(&BlobId("b".into()), "STOP SIGN");
        idx.set_stt(&BlobId("b".into()), "transcribed audio");
        let r = idx.get(&BlobId("b".into())).unwrap();
        assert!(r.corpus().contains("STOP SIGN"));
        assert!(r.corpus().contains("transcribed audio"));
    }

    #[test]
    fn search_matches_caption_and_ocr_case_insensitive() {
        let mut idx = AttachmentIndex::default();
        idx.upsert(&att("a", "Blue Car"));
        idx.upsert(&att("b", "Red Truck"));
        idx.set_ocr(&BlobId("a".into()), "license plate ABC123");
        let hits = idx.search("abc");
        assert_eq!(hits, vec![&BlobId("a".into())]);
    }

    #[test]
    fn empty_query_matches_every_row() {
        let mut idx = AttachmentIndex::default();
        idx.upsert(&att("a", "x"));
        idx.upsert(&att("b", "y"));
        assert_eq!(idx.search("").len(), 2);
    }

    #[test]
    fn corpus_skips_empty_pieces() {
        let row = IndexRow {
            kind: "image".into(),
            mime: "image/png".into(),
            caption: "only caption".into(),
            ocr_text: String::new(),
            stt_text: String::new(),
        };
        assert_eq!(row.corpus(), "only caption");
    }
}
