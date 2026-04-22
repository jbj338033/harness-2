// IMPLEMENTS: D-361
//! Legal search request type + PII gate. The `harness-tools-legal-
//! search` crate consumes [`LegalSearchRequest`]. Any query that
//! looks like an SSN or DOB is redacted before it leaves the daemon.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegalSearchRequest {
    pub matter_id: String,
    pub query: String,
    pub jurisdictions: Vec<String>,
    pub limit: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PiiVerdict {
    Clean,
    Redacted { hits: Vec<String> },
}

/// Returns the verdict and a sanitised query. The sanitised query
/// replaces matched PII with `[REDACTED]` so the search service still
/// gets useful context.
#[must_use]
pub fn redact_request(req: &LegalSearchRequest) -> (PiiVerdict, LegalSearchRequest) {
    let mut hits: Vec<String> = Vec::new();
    let mut q = req.query.clone();

    if let Some(redacted) = redact_ssn(&q) {
        hits.push("ssn".into());
        q = redacted;
    }
    if let Some(redacted) = redact_dob(&q) {
        hits.push("dob".into());
        q = redacted;
    }

    let new_req = LegalSearchRequest {
        query: q,
        ..req.clone()
    };
    let verdict = if hits.is_empty() {
        PiiVerdict::Clean
    } else {
        PiiVerdict::Redacted { hits }
    };
    (verdict, new_req)
}

fn redact_ssn(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut last = 0usize;
    let mut hit = false;
    let mut i = 0usize;
    while i + 11 <= bytes.len() {
        if matches_ssn(&bytes[i..i + 11]) {
            out.push_str(&s[last..i]);
            out.push_str("[REDACTED]");
            i += 11;
            last = i;
            hit = true;
        } else {
            i += 1;
        }
    }
    if hit {
        out.push_str(&s[last..]);
        Some(out)
    } else {
        None
    }
}

fn matches_ssn(window: &[u8]) -> bool {
    window.len() == 11
        && window[..3].iter().all(u8::is_ascii_digit)
        && window[3] == b'-'
        && window[4..6].iter().all(u8::is_ascii_digit)
        && window[6] == b'-'
        && window[7..].iter().all(u8::is_ascii_digit)
}

fn redact_dob(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut last = 0usize;
    let mut hit = false;
    let mut i = 0usize;
    while i + 10 <= bytes.len() {
        if matches_dob(&bytes[i..i + 10]) {
            out.push_str(&s[last..i]);
            out.push_str("[REDACTED]");
            i += 10;
            last = i;
            hit = true;
        } else {
            i += 1;
        }
    }
    if hit {
        out.push_str(&s[last..]);
        Some(out)
    } else {
        None
    }
}

fn matches_dob(window: &[u8]) -> bool {
    window.len() == 10
        && window[..4].iter().all(u8::is_ascii_digit)
        && window[4] == b'-'
        && window[5..7].iter().all(u8::is_ascii_digit)
        && window[7] == b'-'
        && window[8..].iter().all(u8::is_ascii_digit)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(q: &str) -> LegalSearchRequest {
        LegalSearchRequest {
            matter_id: "m1".into(),
            query: q.into(),
            jurisdictions: vec!["us-ca".into()],
            limit: 10,
        }
    }

    #[test]
    fn clean_query_passes_through() {
        let (v, r) = redact_request(&req("malpractice second circuit"));
        assert_eq!(v, PiiVerdict::Clean);
        assert_eq!(r.query, "malpractice second circuit");
    }

    #[test]
    fn ssn_redacted() {
        let (v, r) = redact_request(&req("client 123-45-6789 history"));
        match v {
            PiiVerdict::Redacted { hits } => assert!(hits.iter().any(|h| h == "ssn")),
            PiiVerdict::Clean => panic!("expected redaction"),
        }
        assert!(r.query.contains("[REDACTED]"));
        assert!(!r.query.contains("123-45-6789"));
    }

    #[test]
    fn dob_redacted() {
        let (v, r) = redact_request(&req("born 1980-04-19 plaintiff"));
        match v {
            PiiVerdict::Redacted { hits } => assert!(hits.iter().any(|h| h == "dob")),
            PiiVerdict::Clean => panic!("expected redaction"),
        }
        assert!(!r.query.contains("1980-04-19"));
    }

    #[test]
    fn ssn_and_dob_both_redacted() {
        let (v, _) = redact_request(&req("123-45-6789 born 1980-04-19"));
        match v {
            PiiVerdict::Redacted { hits } => {
                assert!(hits.iter().any(|h| h == "ssn"));
                assert!(hits.iter().any(|h| h == "dob"));
            }
            PiiVerdict::Clean => panic!("expected double redaction"),
        }
    }

    #[test]
    fn near_miss_ssn_not_redacted() {
        let (v, _) = redact_request(&req("phone 555-12-34 number"));
        assert_eq!(v, PiiVerdict::Clean);
    }
}
