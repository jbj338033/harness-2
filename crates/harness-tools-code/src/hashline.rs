use serde::{Deserialize, Serialize};
use std::fmt::Write as _;
use thiserror::Error;

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

#[must_use]
pub fn hash_line(line: &str) -> String {
    let trimmed = line.trim();
    let mut h: u32 = 0x0811_c9dc;
    for b in trimmed.bytes() {
        h = h.rotate_left(5) ^ u32::from(b).wrapping_mul(0x9e37_79b1);
    }
    let byte = (((h >> 16) ^ h) & 0xFF) as u8;
    format!("{byte:02x}")
}

#[must_use]
pub fn annotate(content: &str) -> String {
    let mut out = String::with_capacity(content.len() + content.len() / 8);
    for (i, line) in content.split('\n').enumerate() {
        let is_final_empty_after_newline = i > 0
            && line.is_empty()
            && content.ends_with('\n')
            && i == content.matches('\n').count();
        if is_final_empty_after_newline {
            continue;
        }
        let n = i + 1;
        let h = hash_line(line);
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
    if h_str.len() != 2 || !h_str.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(EditError::MalformedAnchor(anchor.to_string()));
    }
    Ok((n, h_str.to_ascii_lowercase()))
}

pub fn apply_hashline_edit(content: &str, anchors: &[HashlineAnchor]) -> Result<String, EditError> {
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

    let total = raw_lines.len();
    let mut plan: Vec<(usize, String)> = Vec::with_capacity(anchors.len());
    for a in anchors {
        let (n, expected_hash) = parse_line_anchor(&a.line)?;
        if n == 0 || n > total {
            return Err(EditError::OutOfRange { line: n, total });
        }
        let actual = hash_line(raw_lines[n - 1]);
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

    #[test]
    fn hash_is_stable_and_trim_insensitive() {
        assert_eq!(hash_line("foo"), hash_line("foo"));
        assert_eq!(hash_line("  foo  "), hash_line("foo"));
        assert_ne!(hash_line("foo"), hash_line("bar"));
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
        assert_eq!(parse_line_anchor("1:ab").unwrap(), (1, "ab".to_string()));
        assert!(parse_line_anchor("1:ZZ").is_err());
        assert!(parse_line_anchor("abc").is_err());
        assert!(parse_line_anchor("1:abc").is_err());
    }

    #[test]
    fn hashline_edit_applies_valid_anchor() {
        let content = "fn main() {\n    println!(\"hi\");\n}\n";
        let hash2 = hash_line("    println!(\"hi\");");
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
            line: "2:ff".into(),
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
        let good_hash = hash_line("a");
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
        let good = HashlineAnchor {
            line: format!("1:{}", hash_line("a")),
            content: "A".into(),
        };
        let bad = HashlineAnchor {
            line: "2:ff".into(),
            content: "B".into(),
        };
        let err = apply_hashline_edit(content, &[good, bad]).unwrap_err();
        assert!(
            matches!(err, EditError::HashMismatch { .. }),
            "expected HashMismatch, got {err:?}"
        );
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
