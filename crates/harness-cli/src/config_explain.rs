// IMPLEMENTS: D-207
pub struct Entry {
    pub key: &'static str,
    pub default: &'static str,
    pub takes_effect: &'static str,
    pub summary: &'static str,
    pub example: &'static str,
}

const ENTRIES: &[Entry] = &[
    Entry {
        key: "default_model",
        default: "(inferred from registered credentials)",
        takes_effect: "next agent turn",
        summary: "model id used when a session does not pin one explicitly",
        example: "claude-3-5-sonnet-latest",
    },
    Entry {
        key: "network.ws_port",
        default: "8384",
        takes_effect: "next harnessd boot",
        summary: "TCP port the websocket listener binds to for paired devices",
        example: "8384",
    },
    Entry {
        key: "network.ws_host",
        default: "0.0.0.0",
        takes_effect: "next harnessd boot",
        summary: "interface address the websocket listener binds to",
        example: "127.0.0.1",
    },
    Entry {
        key: "network.domain",
        default: "(none)",
        takes_effect: "next certificate issuance",
        summary: "extra DNS name added to the daemon's TLS certificate",
        example: "harness.example.com",
    },
    Entry {
        key: "network.update_endpoint",
        default: "https://harness.dev/releases/latest.json",
        takes_effect: "next update check",
        summary: "URL fetched to detect new harness releases",
        example: "https://internal.example.com/harness.json",
    },
    Entry {
        key: "ollama.endpoint",
        default: "http://localhost:11434",
        takes_effect: "next ollama request",
        summary: "base URL for the local ollama server",
        example: "http://localhost:11434",
    },
    Entry {
        key: "browser.cdp_endpoint",
        default: "(none — launches bundled chromium)",
        takes_effect: "next browser tool invocation",
        summary: "Chrome DevTools Protocol endpoint to attach to instead of spawning chromium",
        example: "ws://127.0.0.1:9222",
    },
    Entry {
        key: "update.latest_known",
        default: "(written by daemon)",
        takes_effect: "informational only",
        summary: "newest harness version observed by the update checker",
        example: "0.4.2",
    },
];

const PREFIX_HINTS: &[(&str, &str, &str)] = &[
    (
        "remote.",
        "remote.<wss-url>.fingerprint | remote.<wss-url>.device_id",
        "per-remote pairing record written by `harness connect`",
    ),
    (
        "anthropic.",
        "anthropic.<setting>",
        "anthropic provider tuning persisted by `harness auth login anthropic`",
    ),
    (
        "openai.",
        "openai.<setting>",
        "openai provider tuning persisted by `harness auth login openai`",
    ),
];

#[must_use]
pub fn lookup(key: &str) -> Option<&'static Entry> {
    ENTRIES.iter().find(|e| e.key == key)
}

#[must_use]
pub fn prefix_hint(key: &str) -> Option<&'static (&'static str, &'static str, &'static str)> {
    PREFIX_HINTS
        .iter()
        .find(|(prefix, _, _)| key.starts_with(prefix))
}

#[must_use]
pub fn all() -> &'static [Entry] {
    ENTRIES
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_keys_have_unique_entries() {
        let mut seen = std::collections::HashSet::new();
        for e in ENTRIES {
            assert!(seen.insert(e.key), "duplicate explain entry: {}", e.key);
        }
    }

    #[test]
    fn lookup_finds_default_model() {
        let entry = lookup("default_model").expect("default_model entry present");
        assert!(!entry.summary.is_empty());
    }

    #[test]
    fn lookup_misses_for_unknown_key() {
        assert!(lookup("does.not.exist").is_none());
    }

    #[test]
    fn prefix_hint_matches_remote_namespace() {
        let hit = prefix_hint("remote.wss://x:8384/.fingerprint");
        assert!(hit.is_some());
    }

    #[test]
    fn prefix_hint_misses_unrelated_key() {
        assert!(prefix_hint("default_model").is_none());
    }

    #[test]
    fn entries_have_no_trailing_period_in_summary() {
        for e in ENTRIES {
            assert!(
                !e.summary.ends_with('.'),
                "summary should not end with period: {}",
                e.key
            );
        }
    }
}
