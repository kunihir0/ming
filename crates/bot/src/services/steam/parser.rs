use crate::services::steam::types::{BanStatus, ProfileVisibility, SteamFriend, SteamProfile};
use anyhow::Result;
use regex::Regex;
use scraper::{Html, Selector};
use std::sync::LazyLock;

static REGEX_DAYS_BAN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"([0-9]+)\s+day\(s\)").expect("valid selector or regex"));
static REGEX_GAME_BANS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"([0-9]+)\s+game ban").expect("valid selector or regex"));

/// Parse Steam profile from HTML
///
/// # Panics
/// Panics if selector compilation fails (should never happen).
///
/// # Errors
/// Returns an error if the HTML cannot be parsed.
#[allow(clippy::too_many_lines, clippy::collapsible_if)]
pub fn parse_profile(html: &str, steam_id64: String) -> Result<SteamProfile> {
    let document = Html::parse_document(html);

    let private_info_sel =
        Selector::parse("div.profile_private_info").expect("valid selector or regex");
    let is_not_setup = document.select(&private_info_sel).any(|el| {
        el.text()
            .collect::<String>()
            .contains("This profile is not yet set up")
    });

    let persona_sel = Selector::parse(".actual_persona_name").expect("valid selector or regex");
    let persona_name = document.select(&persona_sel).next().map_or_else(
        || steam_id64.clone(),
        |el| el.text().collect::<String>().trim().to_string(),
    );

    if is_not_setup {
        return Ok(SteamProfile {
            steam_id64,
            vanity_id: None,
            persona_name,
            real_name: None,
            visibility: ProfileVisibility::NotSetup,
            is_game_details_private: true,
            avatar_url: None,
            level: 0,
            location: None,
            member_since: None,
            bans: BanStatus::default(),
            summary: None,
        });
    }

    let real_name_sel = Selector::parse("bdi").expect("valid selector or regex");
    let real_name = document
        .select(&real_name_sel)
        .next()
        .map(|el| el.text().collect::<String>().trim().to_string())
        .filter(|s| !s.is_empty());

    let avatar_sel =
        Selector::parse(".playerAvatarAutoSizeInner img").expect("valid selector or regex");
    let avatar_url = document
        .select(&avatar_sel)
        .next()
        .and_then(|el| el.value().attr("src").map(ToString::to_string));

    let level_sel = Selector::parse(".friendPlayerLevelNum").expect("valid selector or regex");
    let level = document
        .select(&level_sel)
        .next()
        .and_then(|el| el.text().collect::<String>().trim().parse::<u32>().ok())
        .unwrap_or(0);

    let mut visibility = ProfileVisibility::Public;
    let private_info = document
        .select(&private_info_sel)
        .next()
        .map(|el| el.text().collect::<String>());
    if let Some(info) = &private_info {
        if info.contains("This profile is private") {
            visibility = ProfileVisibility::Private;
        } else if info.contains("Friends Only") {
            visibility = ProfileVisibility::FriendsOnly;
        }
    }

    let games_link_sel = Selector::parse(".profile_item_links a[href$=\"/games/?tab=all\"]")
        .expect("valid selector or regex");
    let has_games_link = document.select(&games_link_sel).next().is_some();
    let is_game_details_private = visibility != ProfileVisibility::Public || !has_games_link;

    let location_sel = Selector::parse(".header_real_name").expect("valid selector or regex");
    // Getting location without the real name is tricky with scraper, but we can look for text nodes
    let location = document
        .select(&location_sel)
        .next()
        .and_then(|el| {
            el.text()
                .filter(|t| !t.trim().is_empty())
                .last()
                .map(|t| t.trim().to_string())
        })
        .filter(|s| s != real_name.as_deref().unwrap_or(""));

    let summary_sel = Selector::parse(".profile_summary").expect("valid selector or regex");
    let summary = document
        .select(&summary_sel)
        .next()
        .map(|el| el.text().collect::<String>().trim().to_string())
        .filter(|s| !s.is_empty());

    let ban_sel = Selector::parse(".profile_ban").expect("valid selector or regex");
    let ban_text = document
        .select(&ban_sel)
        .next()
        .map(|el| el.text().collect::<String>().to_lowercase())
        .unwrap_or_default();

    let mut bans = BanStatus {
        is_vac_banned: ban_text.contains("vac ban"),
        is_community_banned: ban_text.contains("community ban"),
        is_game_banned: ban_text.contains("game ban"),
        game_ban_count: 0,
        days_since_last_ban: 0,
    };

    if let Some(caps) = REGEX_DAYS_BAN.captures(&ban_text) {
        if let Some(days) = caps.get(1) {
            bans.days_since_last_ban = days.as_str().parse().unwrap_or(0);
        }
    }
    if let Some(caps) = REGEX_GAME_BANS.captures(&ban_text) {
        if let Some(count) = caps.get(1) {
            bans.game_ban_count = count.as_str().parse().unwrap_or(0);
        }
    }

    Ok(SteamProfile {
        steam_id64,
        vanity_id: None,
        persona_name,
        real_name,
        visibility,
        is_game_details_private,
        avatar_url,
        level,
        location,
        member_since: None, // Hard to scrape accurately without API
        bans,
        summary,
    })
}

/// Parse friends from HTML
///
/// # Panics
/// Panics if selector compilation fails (should never happen).
///
/// # Errors
/// Returns an error if parsing fails.
#[allow(clippy::collapsible_if)]
pub fn parse_friends(html: &str) -> Result<Vec<SteamFriend>> {
    let document = Html::parse_document(html);
    let mut friends = Vec::new();

    let block_sel = Selector::parse(".friend_block_v2").expect("valid selector or regex");
    let content_sel = Selector::parse(".friend_block_content").expect("valid selector or regex");

    for block in document.select(&block_sel) {
        let steam_id64 = block.value().attr("data-steamid").map(ToString::to_string);

        let persona_name = block.select(&content_sel).next().map(|el| {
            // Getting just the first text node (the name)
            el.text().next().unwrap_or("").trim().to_string()
        });

        if let (Some(id), Some(name)) = (steam_id64, persona_name) {
            if !name.is_empty() {
                friends.push(SteamFriend {
                    steam_id64: id,
                    persona_name: name,
                    friends_since: None,
                });
            }
        }
    }

    Ok(friends)
}
