use harness_core::SessionId;
use harness_proto::Notification;
use harness_session::SessionEvent;
use serde_json::json;

pub fn event_to_notification(session_id: SessionId, ev: &SessionEvent) -> Option<Notification> {
    let sid = session_id.as_uuid().to_string();
    match ev {
        SessionEvent::MessageDelta {
            message_id,
            content,
        } => Some(Notification::new(
            "stream.delta",
            Some(json!({
                "session_id": sid,
                "message_id": message_id.as_uuid().to_string(),
                "content": content,
            })),
        )),
        SessionEvent::MessageDone { message_id } => Some(Notification::new(
            "stream.done",
            Some(json!({
                "session_id": sid,
                "message_id": message_id.as_uuid().to_string(),
            })),
        )),
        SessionEvent::Error { reason } => Some(Notification::new(
            "stream.error",
            Some(json!({
                "session_id": sid,
                "reason": reason,
            })),
        )),
        SessionEvent::AgentStatus { agent_id, status } => Some(Notification::new(
            "agent.status",
            Some(json!({
                "session_id": sid,
                "agent_id": agent_id.as_uuid().to_string(),
                "status": status,
            })),
        )),
        SessionEvent::ToolCallStart {
            message_id,
            tool_call_id,
            name,
            input_preview,
        } => Some(Notification::new(
            "stream.tool_call",
            Some(json!({
                "session_id": sid,
                "message_id": message_id.as_uuid().to_string(),
                "tool_call_id": tool_call_id.as_uuid().to_string(),
                "name": name,
                "input_preview": input_preview,
            })),
        )),
        SessionEvent::ToolCallResult {
            tool_call_id,
            output,
            is_error,
        } => Some(Notification::new(
            "stream.tool_result",
            Some(json!({
                "session_id": sid,
                "tool_call_id": tool_call_id.as_uuid().to_string(),
                "output": output,
                "is_error": is_error,
            })),
        )),
        SessionEvent::ApprovalRequest {
            request_id,
            command,
            pattern,
            reason,
        } => Some(Notification::new(
            "approval.request",
            Some(json!({
                "session_id": sid,
                "request_id": request_id,
                "command": command,
                "pattern": pattern,
                "reason": reason,
            })),
        )),
        SessionEvent::MessageCreated { .. } => None,
    }
}
