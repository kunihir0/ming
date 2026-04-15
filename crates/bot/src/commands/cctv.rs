use crate::db::models::ServerChannel;
use crate::db::schema::server_channels::dsl as sc_dsl;
use crate::{Context, Error};
use diesel::prelude::*;
use poise::futures_util::StreamExt;
use poise::serenity_prelude as serenity;
use std::time::Duration;
use tokio::time::interval;

/// View a live CCTV camera stream
#[poise::command(slash_command)]
#[allow(clippy::too_many_lines)]
pub async fn cctv(
    ctx: Context<'_>,
    #[description = "Camera Identifier (e.g. DOME1)"] identifier: String,
) -> Result<(), Error> {
    let channel_id_str = ctx.channel_id().get().to_string();

    // Determine the server_id from the current channel
    let server_id = {
        let mut conn = ctx.data().db_pool.get()?;
        let channel: Option<ServerChannel> = sc_dsl::server_channels
            .filter(sc_dsl::cctv_channel_id.eq(&channel_id_str))
            .first::<ServerChannel>(&mut conn)
            .optional()?;

        let Some(c) = channel else {
            ctx.say("Error: You must run this command in a paired server's `#cctv` channel.")
                .await?;
            return Ok(());
        };
        c.server_id
    };

    // Get the client
    let client = {
        let clients = ctx.data().rustplus_clients.lock().await;
        clients.get(&server_id).cloned()
    };

    let Some(client) = client else {
        ctx.say("Error: Rust+ client not connected for this server.")
            .await?;
        return Ok(());
    };

    ctx.defer().await?;

    let mut camera = rustplus::camera::Camera::new(client, &identifier);
    let mut rx = camera.subscribe_frames();

    if let Err(e) = camera.subscribe().await {
        ctx.say(format!("Failed to subscribe to camera: {e}"))
            .await?;
        return Ok(());
    }

    // Wait for the first frame
    let Ok(Ok(first_frame)) = tokio::time::timeout(Duration::from_secs(5), rx.recv()).await else {
        let _ = camera.unsubscribe().await;
        ctx.say("Timed out waiting for the first camera frame. Camera might not exist.")
            .await?;
        return Ok(());
    };

    let custom_id = format!("cctv_stop_{}", ctx.id());
    let components = vec![serenity::CreateActionRow::Buttons(vec![
        serenity::CreateButton::new(&custom_id)
            .label("Stop Stream")
            .style(serenity::ButtonStyle::Danger),
    ])];

    // Send the initial message with the first frame
    let attachment = serenity::CreateAttachment::bytes(first_frame, format!("{identifier}.png"));
    let embed = serenity::CreateEmbed::new()
        .title(format!("📷 Live feed for {identifier}"))
        .description("*Click the button below to stop streaming.*")
        .image(format!("attachment://{identifier}.png"));

    let mut msg = ctx
        .send(
            poise::CreateReply::default()
                .embed(embed)
                .attachment(attachment)
                .components(components),
        )
        .await?
        .into_message()
        .await?;

    // Start a background loop to update the message every 1.5 seconds
    let http = ctx.serenity_context().http.clone();
    let mut ticker = interval(Duration::from_millis(1500));
    let mut frame_count = 0;
    let identifier_clone = identifier.clone();

    // Create an interaction collector for the stop button
    let mut collector = serenity::ComponentInteractionCollector::new(ctx.serenity_context())
        .message_id(msg.id)
        .filter(move |mci| mci.data.custom_id == custom_id)
        .stream();

    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    // Drain the channel to get the latest frame
                    let mut latest_frame = None;
                    while let Ok(frame) = rx.try_recv() {
                        latest_frame = Some(frame);
                    }

                    if let Some(frame) = latest_frame {
                        frame_count += 1;
                        let attachment = serenity::CreateAttachment::bytes(frame, format!("{identifier_clone}.png"));
                        let embed = serenity::CreateEmbed::new()
                            .title(format!("📷 Live feed for {identifier_clone}"))
                            .description(format!("Frame {frame_count}\n*Updates approx every 1.5s.*"))
                            .image(format!("attachment://{identifier_clone}.png"));

                        let builder = serenity::EditMessage::new()
                            .embed(embed)
                            .new_attachment(attachment);

                        if let Err(e) = msg.edit(&http, builder).await {
                            tracing::warn!("Failed to edit CCTV frame: {}", e);
                            break;
                        }
                    }
                }
                Some(interaction) = collector.next() => {
                    let _ = interaction.defer(&http).await;
                    break;
                }
                else => {
                    // Collector timed out
                    break;
                }
            }
        }

        let _ = camera.unsubscribe().await;

        let embed = serenity::CreateEmbed::new()
            .title(format!("📷 Stream ended for {identifier_clone}"))
            .description(format!("Total frames: {frame_count}"));
        let builder = serenity::EditMessage::new().embed(embed).components(vec![]); // Remove the button
        let _ = msg.edit(&http, builder).await;
    });
    Ok(())
}
