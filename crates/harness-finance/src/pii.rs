// IMPLEMENTS: D-372
//! Finance PII gate (default ON). Card and account numbers are
//! redacted with `[FIN:<kind>]`. We use Luhn check for card numbers
//! to reject PAN-shaped strings that aren't real PANs.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FinancePiiKind {
    /// Payment card primary account number.
    Pan,
    /// Bank routing+account combo.
    AccountNumber,
}

impl FinancePiiKind {
    #[must_use]
    pub fn token(self) -> &'static str {
        match self {
            FinancePiiKind::Pan => "[FIN:pan]",
            FinancePiiKind::AccountNumber => "[FIN:account]",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinancePiiHit {
    pub kind: FinancePiiKind,
    pub original: String,
}

#[must_use]
pub fn redact_finance_pii(input: &str) -> (String, Vec<FinancePiiHit>) {
    let mut hits = Vec::new();
    let after_pan = scan_and_replace_pan(input, &mut hits);
    let after_acct = scan_and_replace_account(&after_pan, &mut hits);
    (after_acct, hits)
}

fn scan_and_replace_pan(input: &str, hits: &mut Vec<FinancePiiHit>) -> String {
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(input.len());
    let mut i = 0usize;
    while i < bytes.len() {
        if let Some(end) = take_pan_at(bytes, i) {
            let original = input[i..end].to_string();
            if luhn_ok(&original) {
                hits.push(FinancePiiHit {
                    kind: FinancePiiKind::Pan,
                    original,
                });
                out.push_str(FinancePiiKind::Pan.token());
                i = end;
                continue;
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

fn take_pan_at(bytes: &[u8], start: usize) -> Option<usize> {
    let mut digit_count = 0usize;
    let mut i = start;
    while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'-' || bytes[i] == b' ') {
        if bytes[i].is_ascii_digit() {
            digit_count += 1;
        }
        if digit_count > 19 {
            return None;
        }
        i += 1;
    }
    if (13..=19).contains(&digit_count) && start_is_word_boundary(bytes, start) {
        Some(i)
    } else {
        None
    }
}

fn start_is_word_boundary(bytes: &[u8], i: usize) -> bool {
    if !bytes[i].is_ascii_digit() {
        return false;
    }
    if i == 0 {
        return true;
    }
    let prev = bytes[i - 1];
    !prev.is_ascii_digit() && prev != b'-'
}

fn luhn_ok(s: &str) -> bool {
    let digits: Vec<u32> = s.chars().filter_map(|c| c.to_digit(10)).collect();
    if digits.len() < 13 {
        return false;
    }
    let total: u32 = digits
        .iter()
        .rev()
        .enumerate()
        .map(|(i, &d)| {
            if i.is_multiple_of(2) {
                d
            } else {
                double_digit(d)
            }
        })
        .sum();
    total.is_multiple_of(10)
}

fn double_digit(d: u32) -> u32 {
    let x = d * 2;
    if x > 9 { x - 9 } else { x }
}

fn scan_and_replace_account(input: &str, hits: &mut Vec<FinancePiiHit>) -> String {
    let lower = input.to_ascii_lowercase();
    let mut out = String::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if let Some(rel) = lower[i..].find("acct:") {
            let label_start = i + rel;
            out.push_str(&input[i..label_start]);
            let after_label = label_start + 5;
            let mut start = after_label;
            while start < bytes.len() && bytes[start] == b' ' {
                start += 1;
            }
            let mut end = start;
            while end < bytes.len() && (bytes[end].is_ascii_digit() || bytes[end] == b'-') {
                end += 1;
            }
            if end > start {
                hits.push(FinancePiiHit {
                    kind: FinancePiiKind::AccountNumber,
                    original: input[start..end].to_string(),
                });
                out.push_str(&input[label_start..start]);
                out.push_str(FinancePiiKind::AccountNumber.token());
                i = end;
            } else {
                out.push_str(&input[label_start..label_start + 5]);
                i = label_start + 5;
            }
        } else {
            out.push_str(&input[i..]);
            return out;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_visa_pan_redacted() {
        let (out, hits) = redact_finance_pii("payment 4111-1111-1111-1111 today");
        assert!(out.contains("[FIN:pan]"));
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].kind, FinancePiiKind::Pan);
    }

    #[test]
    fn invalid_luhn_left_alone() {
        let (out, hits) = redact_finance_pii("payment 4111-1111-1111-1112 today");
        assert!(out.contains("4111-1111-1111-1112"));
        assert!(hits.is_empty());
    }

    #[test]
    fn account_label_redacted() {
        let (out, hits) = redact_finance_pii("ACCT: 12345-6789 wire");
        assert!(out.contains("[FIN:account]"));
        assert!(hits.iter().any(|h| h.kind == FinancePiiKind::AccountNumber));
    }

    #[test]
    fn benign_text_untouched() {
        let (out, hits) = redact_finance_pii("market summary briefing");
        assert_eq!(out, "market summary briefing");
        assert!(hits.is_empty());
    }
}
