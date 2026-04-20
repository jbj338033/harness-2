use async_trait::async_trait;

#[async_trait]
pub trait LlmTitleSource: Send + Sync {
    async fn suggest(&self, user_message: &str, assistant_reply: Option<&str>) -> Option<String>;
}

pub async fn generate_title<S: LlmTitleSource>(
    source: &S,
    user_message: &str,
    assistant_reply: Option<&str>,
) -> String {
    if let Some(t) = source.suggest(user_message, assistant_reply).await {
        let t = t.trim().trim_matches('"').trim_matches('\'').trim();
        if !t.is_empty() {
            return truncate_title(t);
        }
    }
    TitleFromFirstMessage::derive(user_message)
}

fn truncate_title(raw: &str) -> String {
    const MAX: usize = 60;
    if raw.chars().count() <= MAX {
        return raw.replace('\n', " ").trim().to_string();
    }
    let mut out = String::new();
    for ch in raw.chars() {
        if out.chars().count() >= MAX {
            break;
        }
        if ch == '\n' {
            out.push(' ');
        } else {
            out.push(ch);
        }
    }
    let trimmed = out.trim_end_matches(|c: char| c.is_whitespace() || c == ',' || c == '.');
    trimmed.to_string()
}

pub struct TitleFromFirstMessage;

impl TitleFromFirstMessage {
    #[must_use]
    pub fn derive(user_message: &str) -> String {
        let first_line = user_message.lines().next().unwrap_or(user_message).trim();
        if first_line.is_empty() {
            return "untitled".into();
        }
        let trimmed: String = first_line.chars().take(40).collect();
        if first_line.chars().count() > 40 {
            format!("{trimmed}…")
        } else {
            trimmed
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeLlm(Option<&'static str>);

    #[async_trait]
    impl LlmTitleSource for FakeLlm {
        async fn suggest(
            &self,
            _user_message: &str,
            _assistant_reply: Option<&str>,
        ) -> Option<String> {
            self.0.map(String::from)
        }
    }

    #[tokio::test]
    async fn uses_llm_when_available() {
        let t = generate_title(&FakeLlm(Some("Fix Login Bug")), "login broken", None).await;
        assert_eq!(t, "Fix Login Bug");
    }

    #[tokio::test]
    async fn falls_back_when_llm_returns_none() {
        let t = generate_title(&FakeLlm(None), "fix bug", None).await;
        assert_eq!(t, "fix bug");
    }

    #[tokio::test]
    async fn fallback_truncates_long_input() {
        let t = generate_title(
            &FakeLlm(None),
            "please refactor the authentication module",
            None,
        )
        .await;
        assert!(t.starts_with("please refactor"));
        assert!(t.ends_with('…'));
    }

    #[tokio::test]
    async fn strips_quotes_and_whitespace() {
        let t = generate_title(&FakeLlm(Some("  \"Refactor Auth\"  ")), "x", None).await;
        assert_eq!(t, "Refactor Auth");
    }

    #[test]
    fn heuristic_truncates_long_input() {
        let long = "a".repeat(200);
        let t = TitleFromFirstMessage::derive(&long);
        assert!(t.chars().count() <= 41);
        assert!(t.ends_with('…'));
    }

    #[test]
    fn heuristic_handles_empty() {
        assert_eq!(TitleFromFirstMessage::derive(""), "untitled");
        assert_eq!(TitleFromFirstMessage::derive("   \n"), "untitled");
    }

    #[test]
    fn heuristic_uses_first_line() {
        assert_eq!(
            TitleFromFirstMessage::derive("fix bug\nmore details"),
            "fix bug"
        );
    }
}
