// IMPLEMENTS: D-154
//! Canonical key catalog. D-154 names five categories every user-facing
//! string must fall under; the CI gate that enforces "no new strings
//! outside this list" reads from [`all_keys`].

use crate::Locale;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyCategory {
    Error,
    ApprovalPrompt,
    Notification,
    SystemMessage,
    SettingsLabel,
}

impl KeyCategory {
    #[must_use]
    pub fn prefix(self) -> &'static str {
        match self {
            Self::Error => "error.",
            Self::ApprovalPrompt => "approval_prompt.",
            Self::Notification => "notification.",
            Self::SystemMessage => "system_message.",
            Self::SettingsLabel => "settings_label.",
        }
    }

    pub fn iter() -> impl Iterator<Item = Self> {
        [
            Self::Error,
            Self::ApprovalPrompt,
            Self::Notification,
            Self::SystemMessage,
            Self::SettingsLabel,
        ]
        .into_iter()
    }
}

#[derive(Debug)]
pub enum CategoryError {
    Unknown(String),
}

impl std::fmt::Display for CategoryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unknown(k) => write!(f, "key {k} does not match any documented category"),
        }
    }
}

impl std::error::Error for CategoryError {}

pub fn category_of(key: &str) -> Result<KeyCategory, CategoryError> {
    for cat in KeyCategory::iter() {
        if key.starts_with(cat.prefix()) {
            return Ok(cat);
        }
    }
    Err(CategoryError::Unknown(key.into()))
}

const KEYS: &[(&str, KeyCategory)] = &[
    // approval_prompt.*
    ("approval_prompt.allow_once", KeyCategory::ApprovalPrompt),
    ("approval_prompt.allow_session", KeyCategory::ApprovalPrompt),
    ("approval_prompt.allow_global", KeyCategory::ApprovalPrompt),
    ("approval_prompt.deny", KeyCategory::ApprovalPrompt),
    ("approval_prompt.stop_turn", KeyCategory::ApprovalPrompt),
    // error.*
    ("error.cost_cap_exceeded", KeyCategory::Error),
    ("error.injection_in_untrusted", KeyCategory::Error),
    ("error.tool_denied", KeyCategory::Error),
    // notification.*
    ("notification.daemon_started", KeyCategory::Notification),
    ("notification.session_resumed", KeyCategory::Notification),
    // system_message.*
    (
        "system_message.workspace_untrusted",
        KeyCategory::SystemMessage,
    ),
    (
        "system_message.repeat_loop_detected",
        KeyCategory::SystemMessage,
    ),
    // settings_label.*
    ("settings_label.default_model", KeyCategory::SettingsLabel),
    ("settings_label.network_ws_port", KeyCategory::SettingsLabel),
];

pub fn all_keys() -> impl Iterator<Item = &'static str> {
    KEYS.iter().map(|(k, _)| *k)
}

pub fn keys_in_category(cat: KeyCategory) -> impl Iterator<Item = &'static str> {
    KEYS.iter().filter(move |(_, c)| *c == cat).map(|(k, _)| *k)
}

#[must_use]
pub fn lookup(locale: Locale, key: &str) -> Option<&'static str> {
    let templates = match locale {
        Locale::En => EN,
        Locale::Ko => KO,
        Locale::Ja => JA,
        // D-276: zh/es/fr/de fall back to English until human review lands.
        Locale::Zh | Locale::Es | Locale::Fr | Locale::De => return None,
    };
    templates.iter().find(|(k, _)| *k == key).map(|(_, v)| *v)
}

const EN: &[(&str, &str)] = &[
    ("approval_prompt.allow_once", "Allow once"),
    ("approval_prompt.allow_session", "Allow this session"),
    ("approval_prompt.allow_global", "Allow always (persisted)"),
    ("approval_prompt.deny", "Deny once"),
    (
        "approval_prompt.stop_turn",
        "Stop turn (deny + cancel agent)",
    ),
    (
        "error.cost_cap_exceeded",
        "Cost cap on {tier} exceeded: ${used} / ${cap}",
    ),
    (
        "error.injection_in_untrusted",
        "Injection pattern {pattern_id} found in {source}",
    ),
    (
        "error.tool_denied",
        "Tool {name} was denied by sandbox: {reason}",
    ),
    (
        "notification.daemon_started",
        "harnessd started — protocol v{protocol_version}",
    ),
    (
        "notification.session_resumed",
        "Resumed session {session_id}",
    ),
    (
        "system_message.workspace_untrusted",
        "This workspace is untrusted — run `harness workspace trust` to grant",
    ),
    (
        "system_message.repeat_loop_detected",
        "The same action {action_hash} ran {count} times",
    ),
    ("settings_label.default_model", "Default model"),
    ("settings_label.network_ws_port", "Daemon websocket port"),
];

const KO: &[(&str, &str)] = &[
    ("approval_prompt.allow_once", "한 번만 허용"),
    ("approval_prompt.allow_session", "이 세션에서 허용"),
    ("approval_prompt.allow_global", "항상 허용 (저장됨)"),
    ("approval_prompt.deny", "이번만 거부"),
    (
        "approval_prompt.stop_turn",
        "턴 중지 (거부 + 에이전트 취소)",
    ),
    (
        "error.cost_cap_exceeded",
        "{tier} 비용 한도 초과: ${used} / ${cap}",
    ),
    (
        "error.injection_in_untrusted",
        "{source} 의 신뢰 불가 영역에서 주입 패턴 {pattern_id} 감지",
    ),
    (
        "error.tool_denied",
        "도구 {name} 가 샌드박스에 의해 거부됨: {reason}",
    ),
    (
        "notification.daemon_started",
        "harnessd 시작 — 프로토콜 v{protocol_version}",
    ),
    ("notification.session_resumed", "세션 {session_id} 재개"),
    (
        "system_message.workspace_untrusted",
        "이 워크스페이스는 신뢰되지 않음 — `harness workspace trust` 로 부여",
    ),
    (
        "system_message.repeat_loop_detected",
        "동일 액션 {action_hash} 가 {count} 회 반복됨",
    ),
    ("settings_label.default_model", "기본 모델"),
    ("settings_label.network_ws_port", "데몬 웹소켓 포트"),
];

const JA: &[(&str, &str)] = &[
    ("approval_prompt.allow_once", "1回のみ許可"),
    ("approval_prompt.allow_session", "このセッションのみ許可"),
    ("approval_prompt.allow_global", "常に許可 (永続化)"),
    ("approval_prompt.deny", "今回は拒否"),
    (
        "approval_prompt.stop_turn",
        "ターン停止 (拒否 + エージェント取消)",
    ),
    (
        "error.cost_cap_exceeded",
        "{tier} コスト上限超過: ${used} / ${cap}",
    ),
    (
        "error.injection_in_untrusted",
        "{source} の信頼できない領域で挿入パターン {pattern_id} を検出",
    ),
    (
        "error.tool_denied",
        "ツール {name} がサンドボックスにより拒否: {reason}",
    ),
    (
        "notification.daemon_started",
        "harnessd 起動 — プロトコル v{protocol_version}",
    ),
    (
        "notification.session_resumed",
        "セッション {session_id} を再開",
    ),
    (
        "system_message.workspace_untrusted",
        "このワークスペースは未信頼 — `harness workspace trust` で許可",
    ),
    (
        "system_message.repeat_loop_detected",
        "同一アクション {action_hash} が {count} 回繰り返された",
    ),
    ("settings_label.default_model", "既定モデル"),
    (
        "settings_label.network_ws_port",
        "デーモン WebSocket ポート",
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_key_belongs_to_a_documented_category() {
        for (k, expected) in KEYS {
            let cat = category_of(k).expect("documented");
            assert_eq!(cat, *expected, "wrong category for {k}");
        }
    }

    #[test]
    fn unknown_key_yields_category_error() {
        assert!(category_of("rogue.key").is_err());
    }

    #[test]
    fn en_catalog_covers_every_documented_key() {
        for k in all_keys() {
            assert!(lookup(Locale::En, k).is_some(), "missing en for {k}");
        }
    }

    #[test]
    fn ko_catalog_covers_every_documented_key() {
        for k in all_keys() {
            assert!(lookup(Locale::Ko, k).is_some(), "missing ko for {k}");
        }
    }

    #[test]
    fn ja_catalog_covers_every_documented_key() {
        for k in all_keys() {
            assert!(lookup(Locale::Ja, k).is_some(), "missing ja for {k}");
        }
    }

    #[test]
    fn pending_locales_explicitly_return_none_so_caller_falls_back() {
        for loc in [Locale::Zh, Locale::Es, Locale::Fr, Locale::De] {
            assert!(lookup(loc, "approval_prompt.allow_once").is_none());
        }
    }

    #[test]
    fn keys_in_category_filters_correctly() {
        let approvals: Vec<&str> = keys_in_category(KeyCategory::ApprovalPrompt).collect();
        assert!(approvals.iter().all(|k| k.starts_with("approval_prompt.")));
        assert_eq!(approvals.len(), 5);
    }
}
