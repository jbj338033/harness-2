use crate::{MemoryRecord, Result, Scope, approx_tokens, list_global, list_project};
use rusqlite::Connection;

const BPS_MAX: u32 = 10_000;

pub struct SelectionParams<'a> {
    pub cwd: &'a str,
    pub query: Option<&'a str>,
    pub context_window: usize,
    pub budget_bps: u32,
}

impl<'a> SelectionParams<'a> {
    #[must_use]
    pub fn new(cwd: &'a str, context_window: usize) -> Self {
        Self {
            cwd,
            query: None,
            context_window,
            budget_bps: 1_000,
        }
    }

    #[must_use]
    pub fn budget_tokens(&self) -> usize {
        let bps = usize::try_from(self.budget_bps.min(BPS_MAX)).unwrap_or(usize::MAX);
        let denom = usize::try_from(BPS_MAX).unwrap_or(usize::MAX);
        self.context_window.saturating_mul(bps) / denom
    }
}

pub fn select_for_turn(
    conn: &Connection,
    params: &SelectionParams<'_>,
) -> Result<Vec<MemoryRecord>> {
    let mut candidates: Vec<MemoryRecord> = Vec::new();
    candidates.extend(list_global(conn)?);
    candidates.extend(list_project(conn, params.cwd)?);

    if let Some(q) = params.query {
        let q_tokens = tokenize(q);
        candidates.sort_by(|a, b| {
            let sa = score(&q_tokens, a);
            let sb = score(&q_tokens, b);
            sb.cmp(&sa).then_with(|| b.created_at.cmp(&a.created_at))
        });
    }

    let budget = params.budget_tokens();
    let mut out = Vec::new();
    let mut used = 0usize;
    for m in candidates {
        let cost = approx_tokens(&m.content);
        if used + cost > budget {
            continue;
        }
        used += cost;
        out.push(m);
    }
    Ok(out)
}

fn tokenize(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|t| !t.is_empty())
        .map(str::to_ascii_lowercase)
        .collect()
}

fn score(query_tokens: &[String], m: &MemoryRecord) -> u32 {
    if query_tokens.is_empty() {
        return 0;
    }
    let content_tokens: std::collections::HashSet<String> =
        tokenize(&m.content).into_iter().collect();
    let mut hits: u32 = 0;
    for token in query_tokens {
        if token.len() < 3 {
            continue;
        }
        if content_tokens.contains(token) {
            hits += 1;
        }
    }
    hits
}

#[must_use]
pub fn render_xml(memories: &[MemoryRecord]) -> String {
    use std::fmt::Write;
    if memories.is_empty() {
        return String::new();
    }
    let mut buf = String::from("<memory>\n");
    for m in memories {
        let scope = match &m.scope {
            Scope::Global => "global".to_string(),
            Scope::Project(p) => format!("project:{p}"),
        };
        let escaped = m
            .content
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;");
        writeln!(buf, "  <item scope=\"{scope}\">{escaped}</item>").unwrap();
    }
    buf.push_str("</memory>\n");
    buf
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{NewMemory, Scope as StoreScope, insert};
    use harness_storage::{Database, Writer, WriterHandle};
    use tempfile::NamedTempFile;

    fn setup() -> (NamedTempFile, WriterHandle) {
        let f = NamedTempFile::new().unwrap();
        Database::open(f.path()).unwrap();
        let h = Writer::spawn(f.path()).unwrap();
        (f, h)
    }

    #[tokio::test]
    async fn selection_respects_budget() {
        let (f, w) = setup();
        for i in 0..10 {
            insert(
                &w,
                NewMemory {
                    scope: StoreScope::Global,
                    content: format!("memory-{i} {}", "x".repeat(180)),
                },
            )
            .await
            .unwrap();
        }

        let reader = rusqlite::Connection::open(f.path()).unwrap();
        let mut params = SelectionParams::new("/work", 1_000);
        params.budget_bps = 1_000;
        let picked = select_for_turn(&reader, &params).unwrap();
        assert!(picked.len() < 10, "should drop some memories under budget");
        let total: usize = picked.iter().map(|m| approx_tokens(&m.content)).sum();
        assert!(total <= 100, "total {total} exceeds budget");
    }

    #[tokio::test]
    async fn selection_combines_global_and_project() {
        let (f, w) = setup();
        insert(
            &w,
            NewMemory {
                scope: StoreScope::Global,
                content: "g1".into(),
            },
        )
        .await
        .unwrap();
        insert(
            &w,
            NewMemory {
                scope: StoreScope::Project("/work/proj".into()),
                content: "p1".into(),
            },
        )
        .await
        .unwrap();

        let reader = rusqlite::Connection::open(f.path()).unwrap();
        let picked =
            select_for_turn(&reader, &SelectionParams::new("/work/proj", 100_000)).unwrap();
        let contents: Vec<_> = picked.iter().map(|m| m.content.as_str()).collect();
        assert!(contents.contains(&"g1"));
        assert!(contents.contains(&"p1"));
    }

    #[tokio::test]
    async fn render_xml_escapes_entities() {
        let memories = vec![MemoryRecord {
            id: "id1".into(),
            scope: Scope::Global,
            content: "x < y & z > 0".into(),
            created_at: 0,
        }];
        let xml = render_xml(&memories);
        assert!(xml.contains("x &lt; y &amp; z &gt; 0"));
        assert!(xml.contains("scope=\"global\""));
    }

    #[test]
    fn budget_bps_clamped_to_max() {
        let p = SelectionParams {
            cwd: "/",
            query: None,
            context_window: 1_000,
            budget_bps: 20_000,
        };
        assert_eq!(p.budget_tokens(), 1_000);
    }

    #[tokio::test]
    async fn query_ranks_matching_memories_first() {
        let (f, w) = setup();
        insert(
            &w,
            NewMemory {
                scope: StoreScope::Global,
                content: "prefer pnpm over npm for node projects".into(),
            },
        )
        .await
        .unwrap();
        insert(
            &w,
            NewMemory {
                scope: StoreScope::Global,
                content: "coffee rules the morning".into(),
            },
        )
        .await
        .unwrap();
        insert(
            &w,
            NewMemory {
                scope: StoreScope::Global,
                content: "another pnpm note".into(),
            },
        )
        .await
        .unwrap();

        let reader = rusqlite::Connection::open(f.path()).unwrap();
        let params = SelectionParams {
            cwd: "/ignored",
            query: Some("install pnpm in this repo"),
            context_window: 100_000,
            budget_bps: 5_000,
        };
        let picked = select_for_turn(&reader, &params).unwrap();
        let contents: Vec<&str> = picked.iter().map(|m| m.content.as_str()).collect();
        assert!(contents[0].contains("pnpm"));
        assert!(contents[1].contains("pnpm"));
        assert_eq!(contents[2], "coffee rules the morning");
    }

    #[tokio::test]
    async fn query_ties_break_by_recency() {
        let (f, w) = setup();
        insert(
            &w,
            NewMemory {
                scope: StoreScope::Global,
                content: "old pnpm note".into(),
            },
        )
        .await
        .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        insert(
            &w,
            NewMemory {
                scope: StoreScope::Global,
                content: "new pnpm note".into(),
            },
        )
        .await
        .unwrap();

        let reader = rusqlite::Connection::open(f.path()).unwrap();
        let picked = select_for_turn(
            &reader,
            &SelectionParams {
                cwd: "/",
                query: Some("pnpm"),
                context_window: 100_000,
                budget_bps: 5_000,
            },
        )
        .unwrap();
        assert!(picked[0].content.starts_with("new"));
        assert!(picked[1].content.starts_with("old"));
    }
}
