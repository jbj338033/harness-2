// IMPLEMENTS: D-362
//! PHI detection — HIPAA Safe Harbor 18 identifiers. We implement the
//! classes whose string shape can be matched statically; richer ones
//! (names, employer, comparative geographics) need a full NER model
//! provided by a companion Presidio adapter. Everything scanned here
//! is redacted with `[PHI:<class>]`.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PhiClass {
    Ssn,
    Mrn,
    Email,
    Phone,
    Date,
    Ip,
    Zip,
    DeviceId,
    UrlWithId,
}

impl PhiClass {
    #[must_use]
    pub fn token(self) -> &'static str {
        match self {
            PhiClass::Ssn => "[PHI:ssn]",
            PhiClass::Mrn => "[PHI:mrn]",
            PhiClass::Email => "[PHI:email]",
            PhiClass::Phone => "[PHI:phone]",
            PhiClass::Date => "[PHI:date]",
            PhiClass::Ip => "[PHI:ip]",
            PhiClass::Zip => "[PHI:zip]",
            PhiClass::DeviceId => "[PHI:device_id]",
            PhiClass::UrlWithId => "[PHI:url]",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PhiHit {
    pub class: PhiClass,
    pub original: String,
}

#[must_use]
pub fn redact_phi(input: &str) -> (String, Vec<PhiHit>) {
    let mut hits = Vec::new();
    let mut out = input.to_string();

    for (class, scan) in &[
        (PhiClass::Ssn, scan_ssn as fn(&str) -> Vec<(usize, usize)>),
        (PhiClass::Phone, scan_phone),
        (PhiClass::Email, scan_email),
        (PhiClass::Date, scan_iso_date),
        (PhiClass::Ip, scan_ipv4),
        (PhiClass::Zip, scan_zip5_with_plus4),
        (PhiClass::Mrn, scan_mrn),
    ] {
        let ranges = scan(&out);
        if !ranges.is_empty() {
            out = replace_ranges(&out, &ranges, class.token(), &mut hits, *class);
        }
    }
    (out, hits)
}

fn replace_ranges(
    input: &str,
    ranges: &[(usize, usize)],
    token: &str,
    hits: &mut Vec<PhiHit>,
    class: PhiClass,
) -> String {
    let mut merged: Vec<(usize, usize)> = ranges.to_vec();
    merged.sort_by_key(|r| r.0);
    let mut out = String::with_capacity(input.len());
    let mut cursor = 0usize;
    for (start, end) in merged {
        if start < cursor {
            continue;
        }
        out.push_str(&input[cursor..start]);
        hits.push(PhiHit {
            class,
            original: input[start..end].to_string(),
        });
        out.push_str(token);
        cursor = end;
    }
    out.push_str(&input[cursor..]);
    out
}

fn scan_ssn(s: &str) -> Vec<(usize, usize)> {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i + 11 <= b.len() {
        let w = &b[i..i + 11];
        if w[..3].iter().all(u8::is_ascii_digit)
            && w[3] == b'-'
            && w[4..6].iter().all(u8::is_ascii_digit)
            && w[6] == b'-'
            && w[7..].iter().all(u8::is_ascii_digit)
        {
            out.push((i, i + 11));
            i += 11;
        } else {
            i += 1;
        }
    }
    out
}

fn scan_phone(s: &str) -> Vec<(usize, usize)> {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i + 12 <= b.len() {
        let w = &b[i..i + 12];
        if w[..3].iter().all(u8::is_ascii_digit)
            && w[3] == b'-'
            && w[4..7].iter().all(u8::is_ascii_digit)
            && w[7] == b'-'
            && w[8..].iter().all(u8::is_ascii_digit)
        {
            out.push((i, i + 12));
            i += 12;
        } else {
            i += 1;
        }
    }
    out
}

fn scan_email(s: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let chars: Vec<(usize, char)> = s.char_indices().collect();
    for (idx, (byte_i, c)) in chars.iter().enumerate() {
        if *c == '@' {
            let start = find_email_start(&chars, idx);
            let end = find_email_end(&chars, idx, s.len());
            if end > *byte_i + 1 && start < *byte_i {
                out.push((start, end));
            }
        }
    }
    out
}

fn find_email_start(chars: &[(usize, char)], at_idx: usize) -> usize {
    let mut start = chars[at_idx].0;
    for i in (0..at_idx).rev() {
        let (bi, c) = chars[i];
        if c.is_alphanumeric() || matches!(c, '.' | '_' | '+' | '-') {
            start = bi;
        } else {
            break;
        }
    }
    start
}

fn find_email_end(chars: &[(usize, char)], at_idx: usize, total: usize) -> usize {
    let mut end = total;
    let mut seen_dot = false;
    for (i, &(bi, c)) in chars.iter().enumerate().skip(at_idx + 1) {
        if c == '.' {
            seen_dot = true;
            continue;
        }
        if !(c.is_alphanumeric() || c == '-') {
            end = bi;
            break;
        }
        if i + 1 == chars.len() {
            end = total;
        }
    }
    if seen_dot { end } else { chars[at_idx].0 }
}

fn scan_iso_date(s: &str) -> Vec<(usize, usize)> {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i + 10 <= b.len() {
        let w = &b[i..i + 10];
        if w[..4].iter().all(u8::is_ascii_digit)
            && w[4] == b'-'
            && w[5..7].iter().all(u8::is_ascii_digit)
            && w[7] == b'-'
            && w[8..].iter().all(u8::is_ascii_digit)
        {
            out.push((i, i + 10));
            i += 10;
        } else {
            i += 1;
        }
    }
    out
}

fn scan_ipv4(s: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let start = i;
        let mut octets = 0;
        let mut cur = i;
        while octets < 4 {
            let dstart = cur;
            let mut dlen = 0;
            while cur < bytes.len() && bytes[cur].is_ascii_digit() && dlen < 3 {
                cur += 1;
                dlen += 1;
            }
            if dlen == 0 {
                break;
            }
            if bytes[dstart..cur]
                .iter()
                .map(|b| (b - b'0') as u32)
                .fold(0u32, |a, d| a * 10 + d)
                > 255
            {
                break;
            }
            octets += 1;
            if octets == 4 {
                break;
            }
            if cur >= bytes.len() || bytes[cur] != b'.' {
                break;
            }
            cur += 1;
        }
        if octets == 4 && cur > start {
            out.push((start, cur));
            i = cur;
        } else {
            i += 1;
        }
    }
    out
}

fn scan_zip5_with_plus4(s: &str) -> Vec<(usize, usize)> {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i + 10 <= b.len() {
        let w = &b[i..i + 10];
        if w[..5].iter().all(u8::is_ascii_digit)
            && w[5] == b'-'
            && w[6..].iter().all(u8::is_ascii_digit)
        {
            let prev_ok = i == 0 || !b[i - 1].is_ascii_digit();
            let next_ok = i + 10 == b.len() || !b[i + 10].is_ascii_digit();
            if prev_ok && next_ok {
                out.push((i, i + 10));
                i += 10;
                continue;
            }
        }
        i += 1;
    }
    out
}

fn scan_mrn(s: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let lower = s.to_ascii_lowercase();
    let needle = "mrn:";
    let mut search_from = 0;
    while let Some(rel) = lower[search_from..].find(needle) {
        let after_colon = search_from + rel + needle.len();
        let bytes = s.as_bytes();
        let mut start = after_colon;
        while start < bytes.len() && bytes[start] == b' ' {
            start += 1;
        }
        let mut end = start;
        while end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'-') {
            end += 1;
        }
        if end > start {
            out.push((start, end));
        }
        search_from = end.max(after_colon);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ssn_redacted() {
        let (out, hits) = redact_phi("patient ssn 123-45-6789 seen today");
        assert!(out.contains("[PHI:ssn]"));
        assert_eq!(hits[0].class, PhiClass::Ssn);
    }

    #[test]
    fn email_redacted() {
        let (out, hits) = redact_phi("contact jane.doe+test@example.com for chart");
        assert!(out.contains("[PHI:email]"));
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].class, PhiClass::Email);
    }

    #[test]
    fn ipv4_redacted() {
        let (out, _) = redact_phi("from 192.168.1.42 at login");
        assert!(out.contains("[PHI:ip]"));
        assert!(!out.contains("192.168.1.42"));
    }

    #[test]
    fn iso_date_redacted() {
        let (out, _) = redact_phi("visit 2026-04-19 follow-up");
        assert!(out.contains("[PHI:date]"));
    }

    #[test]
    fn mrn_label_redacted() {
        let (out, hits) = redact_phi("MRN: A123456 chart attached");
        assert!(out.contains("[PHI:mrn]"));
        assert!(hits.iter().any(|h| h.class == PhiClass::Mrn));
    }

    #[test]
    fn zip_plus4_redacted() {
        let (out, _) = redact_phi("zip 90210-1234 resident");
        assert!(out.contains("[PHI:zip]"));
    }

    #[test]
    fn benign_prose_untouched() {
        let (out, hits) = redact_phi("diabetes chronic condition lifestyle notes");
        assert_eq!(out, "diabetes chronic condition lifestyle notes");
        assert!(hits.is_empty());
    }
}
