// IMPLEMENTS: D-148
//! `harness://` URLs that link a code range back to the conversation that
//! produced it. Mirrors the Cursor RFC schema adopted by Cloudflare,
//! Vercel, and Jules so an external editor can resolve cross-tool links
//! without round-tripping through a UI.
//!
//! Format:
//! ```text
//! harness://session/<session-id>?path=<percent-encoded-path>&range=<L1>-<L2>
//! ```
//! `range` is optional; if present it pins inclusive 1-based line numbers.

use std::path::{Path, PathBuf};

const SCHEME_PREFIX: &str = "harness://session/";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversationLink {
    pub session_id: String,
    pub path: Option<PathBuf>,
    pub range: Option<LineRange>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineRange {
    pub start: u32,
    pub end: u32,
}

impl LineRange {
    #[must_use]
    pub fn new(start: u32, end: u32) -> Self {
        let (a, b) = if start <= end {
            (start, end)
        } else {
            (end, start)
        };
        Self { start: a, end: b }
    }
}

#[must_use]
pub fn build_url(session_id: &str, path: Option<&Path>, range: Option<LineRange>) -> String {
    let mut url = format!("{SCHEME_PREFIX}{}", percent_encode_segment(session_id));
    let mut params: Vec<String> = Vec::new();
    if let Some(p) = path {
        params.push(format!("path={}", percent_encode(&p.display().to_string())));
    }
    if let Some(r) = range {
        params.push(format!("range={}-{}", r.start, r.end));
    }
    if !params.is_empty() {
        url.push('?');
        url.push_str(&params.join("&"));
    }
    url
}

#[must_use]
pub fn parse_url(input: &str) -> Option<ConversationLink> {
    let rest = input.strip_prefix(SCHEME_PREFIX)?;
    let (id_part, query_part) = match rest.split_once('?') {
        Some((a, b)) => (a, Some(b)),
        None => (rest, None),
    };
    if id_part.is_empty() {
        return None;
    }
    let session_id = percent_decode(id_part);
    let mut path: Option<PathBuf> = None;
    let mut range: Option<LineRange> = None;
    if let Some(query) = query_part {
        for kv in query.split('&') {
            let (k, v) = match kv.split_once('=') {
                Some(pair) => pair,
                None => continue,
            };
            match k {
                "path" => {
                    let decoded = percent_decode(v);
                    if !decoded.is_empty() {
                        path = Some(PathBuf::from(decoded));
                    }
                }
                "range" => {
                    if let Some((a, b)) = v.split_once('-')
                        && let (Ok(start), Ok(end)) = (a.parse::<u32>(), b.parse::<u32>())
                    {
                        range = Some(LineRange::new(start, end));
                    }
                }
                _ => {}
            }
        }
    }
    Some(ConversationLink {
        session_id,
        path,
        range,
    })
}

fn is_unreserved(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~' | b'/')
}

fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if is_unreserved(b) {
            out.push(b as char);
        } else {
            use std::fmt::Write as _;
            write!(out, "%{b:02X}").unwrap();
        }
    }
    out
}

fn percent_encode_segment(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            use std::fmt::Write as _;
            write!(out, "%{b:02X}").unwrap();
        }
    }
    out
}

fn percent_decode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let (Some(h), Some(l)) = (hex_value(bytes[i + 1]), hex_value(bytes[i + 2]))
        {
            out.push(((h << 4) | l) as char);
            i += 3;
            continue;
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

fn hex_value(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// Conventional location for emitted traces under a data dir per D-148.
#[must_use]
pub fn default_traces_dir(data_root: &Path) -> PathBuf {
    data_root.join("traces")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_url_with_only_session() {
        let url = build_url("abc-123", None, None);
        assert_eq!(url, "harness://session/abc-123");
    }

    #[test]
    fn build_url_with_path_and_range() {
        let url = build_url(
            "sess",
            Some(Path::new("crates/foo/src/lib.rs")),
            Some(LineRange::new(10, 20)),
        );
        assert!(url.contains("harness://session/sess?"));
        assert!(url.contains("path=crates/foo/src/lib.rs"));
        assert!(url.contains("range=10-20"));
    }

    #[test]
    fn round_trip_keeps_path_and_range() {
        let original = build_url(
            "sess-1",
            Some(Path::new("a/b.rs")),
            Some(LineRange::new(3, 5)),
        );
        let parsed = parse_url(&original).unwrap();
        assert_eq!(parsed.session_id, "sess-1");
        assert_eq!(parsed.path, Some(PathBuf::from("a/b.rs")));
        assert_eq!(parsed.range, Some(LineRange::new(3, 5)));
    }

    #[test]
    fn parse_rejects_other_schemes() {
        assert!(parse_url("https://example.com/").is_none());
        assert!(parse_url("harness://thread/foo").is_none());
        assert!(parse_url("harness://session/").is_none());
    }

    #[test]
    fn line_range_normalizes_inverted_input() {
        let r = LineRange::new(20, 10);
        assert_eq!(r.start, 10);
        assert_eq!(r.end, 20);
    }

    #[test]
    fn percent_encode_decodes_back_for_session_id_with_special_chars() {
        let url = build_url("session id+=&", None, None);
        let parsed = parse_url(&url).unwrap();
        assert_eq!(parsed.session_id, "session id+=&");
    }

    #[test]
    fn parse_ignores_unknown_query_keys() {
        let parsed = parse_url("harness://session/x?path=foo&unknown=bar").unwrap();
        assert_eq!(parsed.path, Some(PathBuf::from("foo")));
    }

    #[test]
    fn parse_drops_invalid_range() {
        let parsed = parse_url("harness://session/x?range=oops").unwrap();
        assert!(parsed.range.is_none());
    }

    #[test]
    fn default_traces_dir_uses_traces_subfolder() {
        let dir = default_traces_dir(Path::new("/var/harness"));
        assert_eq!(dir, PathBuf::from("/var/harness/traces"));
    }
}
