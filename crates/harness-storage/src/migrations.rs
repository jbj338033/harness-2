use rusqlite::Connection;
use rusqlite_migration::{M, Migrations};

#[must_use]
pub fn all() -> Migrations<'static> {
    Migrations::new(vec![
        M::up(INITIAL_SCHEMA),
        M::up(MESSAGES_FTS),
        M::up(MESSAGES_KIND),
        M::up(WORKSPACE_TRUST),
        M::up(EVENTS),
    ])
}

pub fn apply(conn: &mut Connection) -> Result<(), rusqlite_migration::Error> {
    all().to_latest(conn)
}

const INITIAL_SCHEMA: &str = r"
-- Key-value configuration (non-secret daemon settings).
CREATE TABLE config (
    key        TEXT PRIMARY KEY,
    value      TEXT NOT NULL
) STRICT;

-- Provider credentials (API keys, OAuth tokens). Plain text, protected
-- by file permissions on harness.db. See docs/layer-0-daemon-lifecycle.md.
CREATE TABLE credentials (
    id         TEXT PRIMARY KEY,
    provider   TEXT NOT NULL,
    kind       TEXT NOT NULL,         -- 'api_key' | 'oauth'
    value      TEXT NOT NULL,
    label      TEXT,
    created_at INTEGER NOT NULL
) STRICT;
CREATE INDEX idx_credentials_provider ON credentials(provider);

-- Paired client devices (ed25519 public keys).
CREATE TABLE devices (
    id            TEXT PRIMARY KEY,
    name          TEXT NOT NULL,
    public_key    BLOB NOT NULL,
    last_seen_at  INTEGER,
    created_at    INTEGER NOT NULL
) STRICT;

-- A unit of work: one conversation scope with a cwd and optional task.
CREATE TABLE sessions (
    id         TEXT PRIMARY KEY,
    title      TEXT,
    cwd        TEXT NOT NULL,
    task       TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
) STRICT;
CREATE INDEX idx_sessions_updated_at ON sessions(updated_at DESC);

-- An execution unit within a session. Root agents have parent_id NULL.
CREATE TABLE agents (
    id            TEXT PRIMARY KEY,
    session_id    TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    parent_id     TEXT REFERENCES agents(id) ON DELETE CASCADE,
    role          TEXT NOT NULL,
    model         TEXT NOT NULL,
    status        TEXT NOT NULL,     -- pending | running | done | failed
    system_prompt TEXT,
    worktree_path TEXT,
    wave          INTEGER,
    iteration     INTEGER NOT NULL DEFAULT 1,
    created_at    INTEGER NOT NULL,
    completed_at  INTEGER
) STRICT;
CREATE INDEX idx_agents_session ON agents(session_id);
CREATE INDEX idx_agents_parent ON agents(parent_id);
CREATE INDEX idx_agents_status ON agents(status);

-- Conversation messages scoped to a single agent.
CREATE TABLE messages (
    id             TEXT PRIMARY KEY,
    agent_id       TEXT NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    role           TEXT NOT NULL,    -- user | assistant | system
    content        TEXT,
    tokens_in      INTEGER,
    tokens_out     INTEGER,
    cost           REAL,
    model          TEXT,
    created_at     INTEGER NOT NULL
) STRICT;
CREATE INDEX idx_messages_agent ON messages(agent_id);
CREATE INDEX idx_messages_created_at ON messages(created_at);

-- Tool calls and their results attached to messages.
CREATE TABLE tool_calls (
    id           TEXT PRIMARY KEY,
    message_id   TEXT NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    name         TEXT NOT NULL,
    input        TEXT NOT NULL,       -- JSON
    output       TEXT,                 -- NULL until the call completes
    exit_code    INTEGER,
    duration_ms  INTEGER,
    created_at   INTEGER NOT NULL
) STRICT;
CREATE INDEX idx_tool_calls_message ON tool_calls(message_id);
CREATE INDEX idx_tool_calls_name ON tool_calls(name);

-- Persistent memory. project NULL = global. Per docs/layer-3-session.md.
CREATE TABLE memory (
    id         TEXT PRIMARY KEY,
    project    TEXT,
    content    TEXT NOT NULL,
    created_at INTEGER NOT NULL
) STRICT;
CREATE INDEX idx_memory_project ON memory(project);

-- Approval whitelist: 'always allow' decisions scoped per session or globally.
-- Addresses the approval-persistence gap from the design review.
CREATE TABLE approvals (
    id          TEXT PRIMARY KEY,
    session_id  TEXT,                 -- NULL = global scope
    pattern     TEXT NOT NULL,        -- regex or literal pattern
    scope       TEXT NOT NULL,        -- 'session' | 'global'
    expires_at  INTEGER,              -- NULL = never
    created_at  INTEGER NOT NULL
) STRICT;
CREATE INDEX idx_approvals_session ON approvals(session_id);
";

const MESSAGES_KIND: &str = r"
-- 'chat' (default): normal user/assistant/system conversation turn.
-- 'skill_attachment': SKILL.md body injected by the activate_skill tool.
--                     Future compaction will preserve these across summaries.
-- Future values: 'plan_attachment', 'async_agent_attachment', …
ALTER TABLE messages ADD COLUMN kind TEXT NOT NULL DEFAULT 'chat';
CREATE INDEX idx_messages_kind ON messages(agent_id, kind);
";

// IMPLEMENTS: D-041
const EVENTS: &str = r"
-- Strict append-only event log. Projections (messages / tool_calls) fold from
-- this stream. No UPDATE / DELETE is performed by the storage API — only
-- INSERT. Schema versioning of bodies lives inside `payload` JSON.
CREATE TABLE events (
    id              TEXT PRIMARY KEY,    -- uuidv7 hex
    session_id      TEXT NOT NULL,
    actor           TEXT NOT NULL,       -- 'human:user' | 'agent:<uuid>' | 'system:<name>' | 'tool:<name>'
    kind            TEXT NOT NULL,       -- perceive | think | act | observe | remember | recall | plan | verify | trigger | cancel | revise | message_user | message_assistant | message_system | tool_call | tool_result
    correlation_id  TEXT,                -- logical grouping (ToT branch, verify-retry, wave)
    causation_id    TEXT,                -- parent event id that triggered this one
    payload         TEXT NOT NULL,       -- JSON body, see SPECS-event-payloads.md
    created_at      INTEGER NOT NULL     -- ms epoch
) STRICT;
CREATE INDEX idx_events_session ON events(session_id, created_at);
CREATE INDEX idx_events_correlation ON events(correlation_id);
CREATE INDEX idx_events_causation ON events(causation_id);
CREATE INDEX idx_events_kind ON events(kind);
";

// IMPLEMENTS: D-205
const WORKSPACE_TRUST: &str = r"
-- Per-directory trust grants. Untrusted workspaces refuse to load
-- AGENTS.md / CLAUDE.md / SKILL.md until the user opts in.
CREATE TABLE workspaces (
    path        TEXT PRIMARY KEY,    -- canonical absolute path
    trusted     INTEGER NOT NULL,    -- 0 | 1
    trusted_at  INTEGER NOT NULL     -- ms epoch when the row was last written
) STRICT;
";

const MESSAGES_FTS: &str = r"
CREATE VIRTUAL TABLE messages_fts USING fts5(
    content,
    content='messages',
    content_rowid='rowid'
);

CREATE TRIGGER messages_fts_insert AFTER INSERT ON messages BEGIN
    INSERT INTO messages_fts(rowid, content) VALUES (new.rowid, new.content);
END;

CREATE TRIGGER messages_fts_delete AFTER DELETE ON messages BEGIN
    INSERT INTO messages_fts(messages_fts, rowid, content)
    VALUES ('delete', old.rowid, old.content);
END;

CREATE TRIGGER messages_fts_update AFTER UPDATE ON messages BEGIN
    INSERT INTO messages_fts(messages_fts, rowid, content)
    VALUES ('delete', old.rowid, old.content);
    INSERT INTO messages_fts(rowid, content) VALUES (new.rowid, new.content);
END;
";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_validate() {
        all().validate().unwrap();
    }

    #[test]
    fn fresh_db_applies_all_migrations() {
        let mut conn = Connection::open_in_memory().unwrap();
        apply(&mut conn).unwrap();

        let version: i64 = conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(version, 5, "expected migration version 5");

        for t in [
            "config",
            "credentials",
            "devices",
            "sessions",
            "agents",
            "messages",
            "tool_calls",
            "memory",
            "approvals",
            "workspaces",
            "events",
        ] {
            let count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                    [t],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(count, 1, "table {t} should exist");
        }
    }

    #[test]
    fn migrations_are_idempotent() {
        let mut conn = Connection::open_in_memory().unwrap();
        apply(&mut conn).unwrap();
        apply(&mut conn).unwrap();
    }

    #[test]
    fn fts_triggers_work() {
        let mut conn = Connection::open_in_memory().unwrap();
        apply(&mut conn).unwrap();

        conn.execute_batch(
            "INSERT INTO sessions (id, cwd, created_at, updated_at) VALUES ('s1', '/tmp', 1, 1);
             INSERT INTO agents (id, session_id, role, model, status, created_at)
                 VALUES ('a1', 's1', 'root', 'test', 'running', 1);
             INSERT INTO messages (id, agent_id, role, content, created_at)
                 VALUES ('m1', 'a1', 'user', 'hello harness world', 1);",
        )
        .unwrap();

        let hits: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM messages_fts WHERE messages_fts MATCH 'harness'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(hits, 1);
    }

    #[test]
    fn messages_kind_defaults_and_filters() {
        let mut conn = Connection::open_in_memory().unwrap();
        apply(&mut conn).unwrap();
        conn.execute_batch(
            "INSERT INTO sessions (id, cwd, created_at, updated_at) VALUES ('s1', '/tmp', 1, 1);
             INSERT INTO agents (id, session_id, role, model, status, created_at)
                 VALUES ('a1', 's1', 'root', 'test', 'running', 1);
             INSERT INTO messages (id, agent_id, role, content, created_at)
                 VALUES ('m1', 'a1', 'user', 'hi', 1);
             INSERT INTO messages (id, agent_id, role, content, kind, created_at)
                 VALUES ('m2', 'a1', 'system', '<skill_content name=\"x\">body</skill_content>', 'skill_attachment', 2);",
        )
        .unwrap();

        let chat_kind: String = conn
            .query_row("SELECT kind FROM messages WHERE id = 'm1'", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(chat_kind, "chat");

        let attachments: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM messages WHERE agent_id = 'a1' AND kind = 'skill_attachment'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(attachments, 1);
    }
}
