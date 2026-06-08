use crate::framework::{CommandRegistry, ReplyTarget, UnifiedContext};
use std::sync::Arc;
use tracing::error;

/// Listens for in-game team chat messages on a single Rust+ connection
/// and dispatches commands prefixed with `@`.
pub async fn run_in_game_listener(
    server_id: i32,
    data: Arc<crate::framework::MinibotData>,
    registry: Arc<CommandRegistry>,
    mut rx: tokio::sync::broadcast::Receiver<rustplus::proto::AppMessage>,
) {
    while let Ok(msg) = rx.recv().await {
        let Some(broadcast) = msg.broadcast else {
            continue;
        };
        let Some(team_msg) = broadcast.team_message else {
            continue;
        };

        let text = &team_msg.message.message;
        if !text.starts_with('@') {
            continue;
        }

        let parts: Vec<&str> = text[1..].split_whitespace().collect();
        if parts.is_empty() {
            continue;
        }

        // `@v search rifle` → cmd_name="search", args=["rifle"]
        let (cmd_name, args) = if parts[0] == "v" && parts.len() > 1 {
            (parts[1], &parts[2..])
        } else {
            (parts[0], &parts[1..])
        };

        let channel_opt = {
            let map = data.reply_channels.lock().await;
            map.get(&server_id).copied()
        };

        let reply_target = if let Some(channel_id) = channel_opt {
            ReplyTarget::Discord { channel_id }
        } else {
            ReplyTarget::InGameChat { server_id }
        };

        let uctx = UnifiedContext {
            server_id,
            data: &data,
            reply_target,
            discord_id: None,
            steam_id: Some(team_msg.message.steam_id.to_string()),
        };

        if let Err(e) = registry.dispatch(cmd_name, &uctx, args).await {
            error!("Command error: {}", e);
            let _ = uctx.reply(&format!("Error: {}", e)).await;
        }
    }
}
