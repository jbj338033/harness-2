// IMPLEMENTS: D-190, D-306
//! Outbound AI disclosure. Two regulatory hooks:
//!
//! - California SB-942: any chatbot reply that could be mistaken for a
//!   human must announce itself.
//! - EU AI Act Article 50: similar duty, plus an obligation that the
//!   notice be in a machine-readable format the user can detect.
//!
//! [`Channel`] enumerates the surfaces Harness might emit on. Channels
//! that are known operator-only (TUI to the agent's own user) skip the
//! prefix; channels that fan out (Slack reply, Discord message, email)
//! always wrap.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Channel {
    /// Local TUI — operator already knows this is the agent.
    Tui,
    /// Inbound webhook responder.
    Webhook,
    /// Slack thread reply.
    Slack,
    /// Discord channel message.
    Discord,
    /// Email reply.
    Email,
    /// Generic SMS / push notification.
    Sms,
    /// Voice call transcript (TTS).
    Voice,
}

impl Channel {
    /// Operator-only channels skip the disclosure — the user already
    /// knows they're talking to the harness daemon.
    #[must_use]
    pub fn is_operator_only(self) -> bool {
        matches!(self, Self::Tui)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Jurisdiction {
    /// Default — short SB-942-compatible notice.
    UsCa,
    /// EU AI Act Article 50 wording.
    Eu,
    /// Korean AI Basic Act notice (one-liner with model_id placeholder).
    Kr,
    /// Catch-all for jurisdictions without a specific rule yet — emits
    /// the most informative notice (EU style).
    Other,
}

/// Build the disclosure prefix for a given channel/jurisdiction. Returns
/// an empty string for operator-only channels so the caller can splice
/// it in unconditionally.
#[must_use]
pub fn render_prefix(channel: Channel, jurisdiction: Jurisdiction) -> String {
    if channel.is_operator_only() {
        return String::new();
    }
    match jurisdiction {
        Jurisdiction::UsCa => "[AI generated]".into(),
        Jurisdiction::Eu => {
            "[AI generated — EU AI Act Art 50: this content was produced by an automated system]"
                .into()
        }
        Jurisdiction::Kr => "[AI 생성 — 한국 AI 기본법 통지]".into(),
        Jurisdiction::Other => "[AI generated content — automated system]".into(),
    }
}

/// Wrap an outbound message body with the appropriate disclosure prefix.
/// No-op for operator-only channels.
#[must_use]
pub fn wrap_outbound(body: &str, channel: Channel, jurisdiction: Jurisdiction) -> String {
    if channel.is_operator_only() {
        return body.to_string();
    }
    let prefix = render_prefix(channel, jurisdiction);
    if body.is_empty() {
        prefix
    } else {
        format!("{prefix} {body}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tui_channel_is_operator_only_and_passes_through() {
        let body = "hello";
        assert_eq!(wrap_outbound(body, Channel::Tui, Jurisdiction::UsCa), body);
    }

    #[test]
    fn slack_us_ca_gets_short_prefix() {
        let out = wrap_outbound("hi", Channel::Slack, Jurisdiction::UsCa);
        assert!(out.starts_with("[AI generated]"));
        assert!(out.ends_with("hi"));
    }

    #[test]
    fn email_eu_gets_long_eu_prefix() {
        let out = wrap_outbound("ok", Channel::Email, Jurisdiction::Eu);
        assert!(out.contains("EU AI Act Art 50"));
    }

    #[test]
    fn voice_kr_gets_korean_prefix() {
        let out = wrap_outbound("안녕", Channel::Voice, Jurisdiction::Kr);
        assert!(out.contains("AI 생성"));
        assert!(out.contains("한국 AI 기본법"));
    }

    #[test]
    fn other_jurisdiction_falls_back_to_informative_notice() {
        let out = wrap_outbound("x", Channel::Webhook, Jurisdiction::Other);
        assert!(out.contains("AI generated content"));
    }

    #[test]
    fn empty_body_yields_just_the_prefix() {
        let out = wrap_outbound("", Channel::Sms, Jurisdiction::UsCa);
        assert_eq!(out, "[AI generated]");
    }

    #[test]
    fn render_prefix_for_tui_is_empty_string() {
        assert!(render_prefix(Channel::Tui, Jurisdiction::Eu).is_empty());
    }
}
