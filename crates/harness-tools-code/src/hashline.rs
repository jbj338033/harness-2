// IMPLEMENTS: D-046
use serde::{Deserialize, Serialize};
use std::fmt::Write as _;
use thiserror::Error;

const ANCHOR_HEX_LEN: usize = 8;
const CONTEXT_BEFORE: usize = 2;
const CONTEXT_AFTER: usize = 2;

#[derive(Debug, Clone, Error)]
pub enum EditError {
    #[error("malformed anchor: {0}")]
    MalformedAnchor(String),
    #[error("anchor {line} out of range (file has {total} lines)")]
    OutOfRange { line: usize, total: usize },
    #[error("hash mismatch at line {line}: expected {expected}, got {actual}")]
    HashMismatch {
        line: usize,
        expected: String,
        actual: String,
    },
    #[error("exact string not found")]
    StringNotFound,
    #[error("exact string is not unique ({count} occurrences)")]
    StringAmbiguous { count: usize },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HashlineAnchor {
    pub line: String,
    pub content: String,
}

fn normalize_line(line: &str) -> String {
    line.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn context_window(lines: &[&str], idx: usize) -> String {
    let start = idx.saturating_sub(CONTEXT_BEFORE);
    let end = idx.saturating_add(CONTEXT_AFTER + 1).min(lines.len());
    let mut buf = String::new();
    for (i, line) in lines[start..end].iter().enumerate() {
        if i > 0 {
            buf.push('\n');
        }
        buf.push_str(&normalize_line(line));
    }
    buf
}

#[must_use]
pub fn hash_anchor(lines: &[&str], idx: usize) -> String {
    let window = context_window(lines, idx);
    let h = blake3::hash(window.as_bytes());
    h.to_hex().chars().take(ANCHOR_HEX_LEN).collect()
}

fn split_into_lines(content: &str) -> (Vec<&str>, bool) {
    let trailing_newline = content.ends_with('\n');
    let raw_lines: Vec<&str> = if content.is_empty() {
        Vec::new()
    } else if trailing_newline {
        let mut v: Vec<&str> = content.split('\n').collect();
        if v.last().is_some_and(|s| s.is_empty()) {
            v.pop();
        }
        v
    } else {
        content.split('\n').collect()
    };
    (raw_lines, trailing_newline)
}

#[must_use]
pub fn annotate(content: &str) -> String {
    let (lines, _) = split_into_lines(content);
    let mut out = String::with_capacity(content.len() + content.len() / 4);
    for (i, line) in lines.iter().enumerate() {
        let n = i + 1;
        let h = hash_anchor(&lines, i);
        writeln!(out, "{n}:{h}|{line}").unwrap();
    }
    out
}

pub fn parse_line_anchor(anchor: &str) -> Result<(usize, String), EditError> {
    let (n_str, h_str) = anchor
        .split_once(':')
        .ok_or_else(|| EditError::MalformedAnchor(anchor.to_string()))?;
    let n: usize = n_str
        .parse()
        .map_err(|_| EditError::MalformedAnchor(anchor.to_string()))?;
    if h_str.len() != ANCHOR_HEX_LEN || !h_str.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(EditError::MalformedAnchor(anchor.to_string()));
    }
    Ok((n, h_str.to_ascii_lowercase()))
}

pub fn apply_hashline_edit(content: &str, anchors: &[HashlineAnchor]) -> Result<String, EditError> {
    let (raw_lines, trailing_newline) = split_into_lines(content);
    let total = raw_lines.len();
    let mut plan: Vec<(usize, String)> = Vec::with_capacity(anchors.len());
    for a in anchors {
        let (n, expected_hash) = parse_line_anchor(&a.line)?;
        if n == 0 || n > total {
            return Err(EditError::OutOfRange { line: n, total });
        }
        let actual = hash_anchor(&raw_lines, n - 1);
        if actual != expected_hash {
            return Err(EditError::HashMismatch {
                line: n,
                expected: expected_hash,
                actual,
            });
        }
        plan.push((n, a.content.clone()));
    }

    let mut out: Vec<String> = raw_lines.iter().map(|s| (*s).to_string()).collect();
    for (n, new) in plan {
        out[n - 1] = new;
    }

    let mut s = out.join("\n");
    if trailing_newline {
        s.push('\n');
    }
    Ok(s)
}

pub fn apply_string_replace(
    content: &str,
    old: &str,
    new: &str,
    replace_all: bool,
) -> Result<String, EditError> {
    let count = content.matches(old).count();
    if count == 0 {
        return Err(EditError::StringNotFound);
    }
    if !replace_all && count > 1 {
        return Err(EditError::StringAmbiguous { count });
    }
    Ok(if replace_all {
        content.replace(old, new)
    } else {
        content.replacen(old, new, 1)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn anchor_for(content: &str, line_one_based: usize) -> String {
        let (lines, _) = split_into_lines(content);
        hash_anchor(&lines, line_one_based - 1)
    }

    #[test]
    fn hash_uses_context_window() {
        // Same target line in different contexts should produce different anchors.
        let h_a = anchor_for("alpha\nbeta\ngamma\n", 2);
        let h_b = anchor_for("alpha\nbeta\ndelta\n", 2);
        assert_ne!(h_a, h_b, "context window must influence anchor");
    }

    #[test]
    fn hash_is_whitespace_normalized_within_window() {
        let h_a = anchor_for("alpha\n  beta  \ngamma\n", 2);
        let h_b = anchor_for("alpha\nbeta\ngamma\n", 2);
        assert_eq!(h_a, h_b, "leading/trailing whitespace must be normalized");
    }

    #[test]
    fn hash_clamps_at_file_boundaries() {
        // first line: only itself + 2 after exist
        let one = anchor_for("a\nb\nc\nd\n", 1);
        let three = anchor_for("a\nb\nc\nd\n", 3);
        assert_ne!(one, three);
    }

    #[test]
    fn annotate_prepends_anchor_to_each_line() {
        let out = annotate("a\nb\n");
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].ends_with("|a"));
        assert!(lines[1].ends_with("|b"));
        assert!(lines[0].starts_with("1:"));
        assert!(lines[1].starts_with("2:"));
    }

    #[test]
    fn parse_anchor_ok_and_malformed() {
        assert_eq!(
            parse_line_anchor("1:abcdef01").unwrap(),
            (1, "abcdef01".to_string())
        );
        assert!(parse_line_anchor("1:ZZ").is_err());
        assert!(parse_line_anchor("abc").is_err());
        assert!(parse_line_anchor("1:abc").is_err());
        // legacy 2-char anchors no longer accepted
        assert!(parse_line_anchor("1:ab").is_err());
    }

    #[test]
    fn hashline_edit_applies_valid_anchor() {
        let content = "fn main() {\n    println!(\"hi\");\n}\n";
        let hash2 = anchor_for(content, 2);
        let edit = HashlineAnchor {
            line: format!("2:{hash2}"),
            content: "    println!(\"hello\");".into(),
        };
        let out = apply_hashline_edit(content, &[edit]).unwrap();
        assert!(out.contains("hello"));
        assert!(!out.contains("hi\""));
        assert!(out.ends_with('\n'));
    }

    #[test]
    fn hashline_edit_rejects_stale_hash() {
        let content = "a\nb\nc\n";
        let edit = HashlineAnchor {
            line: "2:ffffffff".into(),
            content: "B".into(),
        };
        let err = apply_hashline_edit(content, &[edit]).unwrap_err();
        assert!(
            matches!(err, EditError::HashMismatch { line: 2, .. }),
            "expected HashMismatch, got {err:?}"
        );
    }

    #[test]
    fn hashline_edit_rejects_out_of_range() {
        let content = "a\nb\n";
        let good_hash = anchor_for(content, 1);
        let edit = HashlineAnchor {
            line: format!("5:{good_hash}"),
            content: "Z".into(),
        };
        let err = apply_hashline_edit(content, &[edit]).unwrap_err();
        assert!(matches!(err, EditError::OutOfRange { line: 5, total: 2 }));
    }

    #[test]
    fn hashline_edit_is_atomic_on_failure() {
        let content = "a\nb\nc\n";
        let good_hash = anchor_for(content, 1);
        let good = HashlineAnchor {
            line: format!("1:{good_hash}"),
            content: "A".into(),
        };
        let bad = HashlineAnchor {
            line: "2:ffffffff".into(),
            content: "B".into(),
        };
        let err = apply_hashline_edit(content, &[good, bad]).unwrap_err();
        assert!(
            matches!(err, EditError::HashMismatch { .. }),
            "expected HashMismatch, got {err:?}"
        );
        // Verify nothing changed: re-applying just the good anchor keeps line 2 unchanged
        let out = apply_hashline_edit(content, &[]).unwrap();
        assert_eq!(out, content);
    }

    #[test]
    fn hashline_no_fuzzy_fallback() {
        // Even when only the target line text matches, anchor must mismatch
        // because the surrounding context differs.
        let original = "alpha\nbeta\ngamma\n";
        let anchor_orig = anchor_for(original, 2);
        let edited = "alpha\nbeta\ndelta\n";
        let edit = HashlineAnchor {
            line: format!("2:{anchor_orig}"),
            content: "BETA".into(),
        };
        let err = apply_hashline_edit(edited, &[edit]).unwrap_err();
        assert!(matches!(err, EditError::HashMismatch { .. }));
    }

    #[test]
    fn string_replace_unique() {
        let out = apply_string_replace("foo bar foo", "bar", "baz", false).unwrap();
        assert_eq!(out, "foo baz foo");
    }

    #[test]
    fn string_replace_rejects_ambiguous() {
        let err = apply_string_replace("foo bar foo", "foo", "baz", false).unwrap_err();
        assert!(matches!(err, EditError::StringAmbiguous { count: 2 }));
    }

    #[test]
    fn string_replace_all() {
        let out = apply_string_replace("foo foo foo", "foo", "bar", true).unwrap();
        assert_eq!(out, "bar bar bar");
    }

    #[test]
    fn string_replace_not_found() {
        let err = apply_string_replace("hello", "xyz", "abc", false).unwrap_err();
        assert!(matches!(err, EditError::StringNotFound));
    }
}
