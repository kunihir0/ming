use anyhow::Result;
use base64::{engine::general_purpose, Engine as _};
use reqwest::Client;
use serde_json::json;
use songbird::input::Input;
use std::sync::Arc;
use tracing::info;

const API_BASE: &str = "https://tiktok-tts-aio.exampleuser.workers.dev/api/generate";

/// Generates TTS audio via TikTok TTS API and returns the MP3 bytes.
pub async fn generate_tts(text: &str, voice: &str, http_client: &Client) -> Result<Vec<u8>> {
    let payload = json!({
        "text": text,
        "voice": voice,
        "base64": true
    });

    let resp = http_client
        .post(API_BASE)
        .json(&payload)
        .send()
        .await?
        .error_for_status()?;

    let b64_str = resp.text().await?;
    let decoded = general_purpose::STANDARD.decode(b64_str.trim())?;
    Ok(decoded)
}

/// Joins a voice channel, plays the provided audio bytes, and leaves after playback finishes.
pub async fn play_and_leave(
    songbird_manager: Arc<songbird::Songbird>,
    guild_id: serenity::model::id::GuildId,
    channel_id: serenity::model::id::ChannelId,
    audio_bytes: Vec<u8>,
) -> Result<()> {
    info!("Joining voice channel {} in guild {}", channel_id, guild_id);

    // Join the voice channel
    let handler_lock = songbird_manager.join(guild_id, channel_id).await?;
    let mut handler = handler_lock.lock().await;

    // Create an input from the memory bytes
    let input: Input = audio_bytes.into();

    // Play the input
    let track_handle = handler.play_input(input);
    let _ = track_handle.set_volume(0.5);

    // Drop the lock so playback can happen
    drop(handler);

    // Wait for the track to finish playing
    while track_handle.get_info().await.is_ok() {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    // Leave the channel after a short delay
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    songbird_manager.leave(guild_id).await?;
    info!("Left voice channel {} in guild {}", channel_id, guild_id);

    Ok(())
}
