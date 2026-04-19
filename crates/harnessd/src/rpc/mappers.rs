use harness_auth::pairing::DeviceRecord;
use harness_session::{AgentRecord, MessageRecord, SessionRecord, message::MessageRole};
use serde_json::{Value, json};

pub fn session_to_json(s: &SessionRecord) -> Value {
    json!({
        "id": s.id.as_uuid().to_string(),
        "title": s.title,
        "cwd": s.cwd,
        "task": s.task,
        "created_at": s.created_at,
        "updated_at": s.updated_at,
    })
}

pub fn agent_to_json(a: &AgentRecord) -> Value {
    json!({
        "id": a.id.as_uuid().to_string(),
        "session_id": a.session_id.as_uuid().to_string(),
        "parent_id": a.parent_id.map(|p| p.as_uuid().to_string()),
        "role": a.role,
        "model": a.model,
        "status": a.status.as_str(),
        "worktree_path": a.worktree_path,
        "iteration": a.iteration,
        "created_at": a.created_at,
        "completed_at": a.completed_at,
    })
}

pub fn message_to_json(m: &MessageRecord) -> Value {
    json!({
        "id": m.id.as_uuid().to_string(),
        "agent_id": m.agent_id.as_uuid().to_string(),
        "role": match m.role {
            MessageRole::User => "user",
            MessageRole::Assistant => "assistant",
            MessageRole::System => "system",
        },
        "content": m.content,
        "tokens_in": m.tokens_in,
        "tokens_out": m.tokens_out,
        "cost": m.cost,
        "model": m.model,
        "created_at": m.created_at,
    })
}

pub fn device_to_json(d: &DeviceRecord) -> Value {
    json!({
        "id": d.id,
        "name": d.name,
        "last_seen_at": d.last_seen_at,
        "created_at": d.created_at,
    })
}

pub fn preview_secret(s: &str) -> String {
    let n = s.len();
    if n <= 8 {
        return "***".into();
    }
    format!("{}…{}", &s[..4], &s[n - 4..])
}
