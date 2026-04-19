use crate::app::App;
use anyhow::Result;
use harness_proto::Request;
use serde_json::{Value, json};
use tokio::io::AsyncWriteExt;
use tokio::net::unix::OwnedWriteHalf;

pub async fn send_chat(
    app: &mut App,
    writer: Option<&mut OwnedWriteHalf>,
    message: String,
) -> Result<()> {
    let Some(w) = writer else {
        app.push_error("not connected");
        return Ok(());
    };
    let Some(sid) = app.session_id.clone() else {
        app.pending_chat = Some(message);
        app.push_system("queued — will send when the session is ready");
        return Ok(());
    };
    let id = app.next_id();
    let mut params = json!({ "session_id": sid, "message": message });
    if let Some(m) = app.model.as_ref() {
        params["model"] = Value::String(m.clone());
    }
    let req = Request::new(id, "v1.chat.send", Some(params));
    let text = serde_json::to_string(&req)?;
    w.write_all(text.as_bytes()).await?;
    w.write_all(b"\n").await?;
    app.turn_running = true;
    Ok(())
}
