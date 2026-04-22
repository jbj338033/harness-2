// IMPLEMENTS: D-171
//! Harness-owned, dependency-free pattern library — looks for code smells
//! the verify loop should reject before they ship. Substring matching only;
//! we deliberately avoid regex to keep the supply chain at zero (D-171c).

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pattern {
    pub id: &'static str,
    pub needle: &'static str,
    pub summary: &'static str,
}

const PATTERNS: &[Pattern] = &[
    Pattern {
        id: "rust.todo",
        needle: "todo!()",
        summary: "todo!() macro is a placeholder bomb",
    },
    Pattern {
        id: "rust.unimplemented",
        needle: "unimplemented!()",
        summary: "unimplemented!() macro is a placeholder bomb",
    },
    Pattern {
        id: "rust.allow_attr",
        needle: "#[allow(",
        summary: "#[allow(...)] hides a real lint — fix the lint instead",
    },
    Pattern {
        id: "rust.let_underscore_result",
        needle: "let _ = ",
        summary: "let _ = ... silently swallows a Result",
    },
    Pattern {
        id: "shell.curl_pipe_sh",
        needle: "curl",
        summary: "curl … | sh / bash — never pipe untrusted bytes into a shell",
    },
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    pub id: &'static str,
    pub summary: &'static str,
    pub byte_offset: usize,
}

#[must_use]
pub fn scan(text: &str) -> Vec<Hit> {
    let mut out = Vec::new();
    for p in PATTERNS {
        if p.id == "shell.curl_pipe_sh" {
            // Refine: only hit when the curl line actually pipes into a shell.
            for (line_offset, line) in line_offsets(text) {
                if line.contains("curl") && (line.contains("| sh") || line.contains("| bash")) {
                    out.push(Hit {
                        id: p.id,
                        summary: p.summary,
                        byte_offset: line_offset,
                    });
                    break;
                }
            }
            continue;
        }
        if let Some(idx) = text.find(p.needle) {
            out.push(Hit {
                id: p.id,
                summary: p.summary,
                byte_offset: idx,
            });
        }
    }
    out
}

fn line_offsets(text: &str) -> impl Iterator<Item = (usize, &str)> {
    let mut offset = 0;
    text.split_inclusive('\n').map(move |line| {
        let here = offset;
        offset += line.len();
        (here, line.trim_end_matches('\n'))
    })
}

#[must_use]
pub fn library() -> &'static [Pattern] {
    PATTERNS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_text_has_no_hits() {
        assert!(scan("").is_empty());
        assert!(scan("safe text").is_empty());
    }

    #[test]
    fn detects_todo_macro() {
        let hits = scan("fn main() { todo!() }");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id, "rust.todo");
    }

    #[test]
    fn detects_let_underscore() {
        let hits = scan("let _ = something();");
        assert!(hits.iter().any(|h| h.id == "rust.let_underscore_result"));
    }

    #[test]
    fn detects_allow_attr() {
        let hits = scan("#[allow(dead_code)]\nfn f() {}");
        assert!(hits.iter().any(|h| h.id == "rust.allow_attr"));
    }

    #[test]
    fn curl_pipe_sh_caught_only_when_actually_piped() {
        // Plain curl without pipe should NOT trip the shell rule.
        assert!(
            !scan("curl https://example.com -o file")
                .iter()
                .any(|h| h.id == "shell.curl_pipe_sh")
        );
        // Pipe to bash trips it.
        assert!(
            scan("curl https://example.com | bash")
                .iter()
                .any(|h| h.id == "shell.curl_pipe_sh")
        );
        assert!(
            scan("curl https://example.com | sh")
                .iter()
                .any(|h| h.id == "shell.curl_pipe_sh")
        );
    }

    #[test]
    fn library_ids_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for p in library() {
            assert!(seen.insert(p.id), "duplicate pattern id: {}", p.id);
        }
    }
}
