use crate::framework::MinibotData;
use std::sync::Arc;
use db::{upsert_player_link, get_bm_id_for_steam_id};

use team_dec::services::steam::SteamService;
use scraper::{Html, Selector};

type Error = Box<dyn std::error::Error + Send + Sync>;
type PoiseContext<'a> = poise::Context<'a, Arc<MinibotData>, Error>;

pub async fn get_player_hours_text(
    db_pool: &db::DbPool,
    steam_id: String,
    bm_id: Option<String>
) -> Result<String, anyhow::Error> {
    let mut conn = db_pool.get()?;
    
    // 1. Link accounts if bm_id is provided
    let final_bm_id = if let Some(ref new_bm_id) = bm_id {
        if !new_bm_id.is_empty() {
            upsert_player_link(&mut conn, &steam_id, new_bm_id)?;
            Some(new_bm_id.clone())
        } else {
            get_bm_id_for_steam_id(&mut conn, &steam_id)?
        }
    } else {
        // Try to fetch from DB
        get_bm_id_for_steam_id(&mut conn, &steam_id)?
    };

    // 2. Fetch Steam hours
    let mut steam_hours_text = "Private / Unknown".to_string();
    
    let steam_service = SteamService::new(false);
    if let Ok(content) = steam_service.get_profile_content_by_steam_id(&steam_id).await {
        let doc = Html::parse_document(&content);
        let game_sel = Selector::parse(".game_info_details").unwrap();
        let name_sel = Selector::parse(".game_name").unwrap();
        let hours_sel = Selector::parse(".hours_played").unwrap();

        for game in doc.select(&game_sel) {
            if let Some(name_el) = game.select(&name_sel).next() {
                let name = name_el.text().collect::<String>();
                if name.to_lowercase().contains("rust") {
                    if let Some(hours_el) = game.select(&hours_sel).next() {
                        steam_hours_text = hours_el.text().collect::<String>().trim().to_string();
                    }
                    break;
                }
            }
        }
    }

    // 3. Fetch BM hours
    let mut bm_hours_text = "Not Linked (Provide BM ID to link)".to_string();

    if let Some(ref bid) = final_bm_id {
        let bm_client = crate::tracking::battlemetrics::client::BmScraperClient::new();
        if let Ok(bm_player) = bm_client.scrape_player_profile(bid).await {
            let total_hours = bm_player.total_playtime_seconds / 3600;
            bm_hours_text = format!("{} hrs", total_hours);
        } else {
            bm_hours_text = format!("Linked (ID: {}). Failed to scrape.", bid);
        }
    }

    let bm_display = final_bm_id.as_deref().unwrap_or("None");

    let response_text = format!(
        "```asciidoc\n\
        = Player Hours =\n\
        \n\
        * Steam ID: {}\n\
        * BattleMetrics ID: {}\n\
        \n\
        [Hours]\n\
        * Steam Rust Hours: {}\n\
        * BattleMetrics Hours: {}\n\
        \n\
        Note: Data may be hidden if profile is private.\n\
        ```",
        steam_id, bm_display, steam_hours_text, bm_hours_text
    );

    Ok(response_text)
}

/// Check a player's Rust hours (Steam and BattleMetrics)
#[poise::command(slash_command, category = "Player Tracking")]
pub async fn hours(
    ctx: PoiseContext<'_>,
    #[description = "Steam ID 64"] steam_id: String,
    #[description = "BattleMetrics ID (Optional, links accounts if provided)"] bm_id: Option<String>,
) -> Result<(), Error> {
    ctx.defer().await?;

    let response_text = get_player_hours_text(&ctx.data().db_pool, steam_id, bm_id).await?;

    ctx.send(poise::CreateReply::default().content(response_text)).await?;

    Ok(())
}
