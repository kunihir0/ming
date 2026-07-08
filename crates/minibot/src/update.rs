use crate::{Error, PoiseContext};

use std::env;
use tracing::{error, info};

/// Update the bot to the latest version published on GitHub Releases
#[poise::command(
    slash_command,
    required_permissions = "ADMINISTRATOR",
    category = "System"
)]
pub async fn update_bot(ctx: PoiseContext<'_>) -> Result<(), Error> {
    ctx.defer_ephemeral().await?;

    let repo_owner = env::var("GITHUB_REPO_OWNER").unwrap_or_else(|_| "kunihir0".to_string());
    let repo_name = env::var("GITHUB_REPO_NAME").unwrap_or_else(|_| "ming".to_string());

    ctx.say(format!(
        "Checking for updates from {}/{}...",
        repo_owner, repo_name
    ))
    .await?;

    let mut builder = self_update::backends::github::Update::configure();
    builder
        .repo_owner(&repo_owner)
        .repo_name(&repo_name)
        .bin_name("minibot-x86_64-linux-gnu") // The asset name attached to the release
        .show_download_progress(false)
        .current_version(env!("CARGO_PKG_VERSION"));

    // If a private repo, we need a token
    if let Ok(token) = env::var("GITHUB_TOKEN") {
        builder.auth_token(&token);
    }

    let status = match tokio::task::spawn_blocking(move || builder.build()?.update()).await {
        Ok(Ok(status)) => status,
        Ok(Err(e)) => {
            error!("Update failed: {}", e);
            ctx.say(format!("❌ Update failed: {}", e)).await?;
            return Ok(());
        }
        Err(e) => {
            error!("Update thread panicked: {}", e);
            ctx.say("❌ Update process panicked.").await?;
            return Ok(());
        }
    };

    match status {
        self_update::Status::Updated(v) => {
            ctx.say(format!(
                "✅ Successfully updated to version `{}`! Please restart the bot process.",
                v
            ))
            .await?;
            info!("Bot updated to version {}. Requires restart.", v);
        }
        self_update::Status::UpToDate(_v) => {
            ctx.say("✅ The bot is already up-to-date or no new release was found.")
                .await?;
        }
    }

    Ok(())
}
