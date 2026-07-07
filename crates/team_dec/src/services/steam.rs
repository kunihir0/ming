use std::sync::LazyLock;
use reqwest::Client;
use scraper::{Html, Selector};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

use crate::error::{Result, TeamDetectorError};
use crate::models::Player;

static STEAM_ID_REGEX: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r#","steamid":"(.*?)","#).unwrap());
static CUSTOM_ID_REGEX: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r#"g_rgProfileData = \{"url":"https://steamcommunity\.com/id/(.*?)/""#)
        .unwrap()
});
static COMMENTS_REGEX: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r#"InitializeCommentThread\(.*"total_count":(\d+),"#).unwrap());
static FRIEND_HREF_REGEX: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r#"id/(.*?)(/|$)"#).unwrap());
static COMMENT_STEAM_ID_REGEX: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r#"profiles/(.*?)(/|$)"#).unwrap());

static SELECTOR_PERSONA_NAME: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse(".actual_persona_name").unwrap());
static SELECTOR_IN_GAME_NAME: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse(".profile_in_game_name").unwrap());
static SELECTOR_IN_GAME_HEADER: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse(".profile_in_game_header").unwrap());
static SELECTOR_COMMENT_SPAN: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("span[id^='commentthread_profile_'][id$='_totalcount']").unwrap());
static SELECTOR_FRIEND_BLOCK: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse(".friend_block_v2").unwrap());
static SELECTOR_FRIEND_LINK: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("a.friend_block_link_overlay").unwrap());
static SELECTOR_FRIEND_CONTENT: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse(".friend_block_content").unwrap());
static SELECTOR_AUTHOR_LINK: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse(".commentthread_author_link").unwrap());
static SELECTOR_BDI: LazyLock<Selector> = LazyLock::new(|| Selector::parse("bdi").unwrap());

pub struct SteamService {
    client: Client,
    debug: bool,
    request_delay: Duration,
    last_request_time: Arc<tokio::sync::Mutex<Instant>>,
    // Caches
    steam_profiles: Arc<RwLock<HashMap<String, String>>>,
    steam_friends: Arc<RwLock<HashMap<String, String>>>,
    custom_id_translation: Arc<RwLock<HashMap<String, String>>>,
}

impl SteamService {
    pub fn new(debug: bool) -> Self {
        Self {
            client: Client::builder()
                .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
                .build()
                .unwrap_or_else(|_| Client::new()),
            debug,
            request_delay: Duration::from_secs(4),
            last_request_time: Arc::new(tokio::sync::Mutex::new(Instant::now() - Duration::from_secs(4))),
            steam_profiles: Arc::new(RwLock::new(HashMap::new())),
            steam_friends: Arc::new(RwLock::new(HashMap::new())),
            custom_id_translation: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    async fn request(&self, url: &str) -> Result<String> {
        let wait_time = {
            let mut last_req = self.last_request_time.lock().await;
            let now = Instant::now();
            let target = *last_req + self.request_delay;

            if target > now {
                let wait = target.duration_since(now);
                *last_req = target;
                Some(wait)
            } else {
                *last_req = now;
                None
            }
        };

        if let Some(wait) = wait_time {
            if self.debug {
                tracing::debug!(wait_ms = wait.as_millis(), "Rate limiting: Waiting...");
            }
            tokio::time::sleep(wait).await;
        }

        if self.debug {
            tracing::debug!(url = %url, "Requesting");
        }
        let response = self.client.get(url).send().await?.error_for_status()?;
        let text = response.text().await?;
        Ok(text)
    }

    fn url_profile_by_steam_id(&self, steam_id: &str) -> String {
        format!(
            "https://steamcommunity.com/profiles/{}/?l=english",
            steam_id
        )
    }

    fn url_profile_by_custom_id(&self, custom_id: &str) -> String {
        format!("https://steamcommunity.com/id/{}/?l=english", custom_id)
    }

    fn url_friends_by_steam_id(&self, steam_id: &str) -> String {
        format!(
            "https://steamcommunity.com/profiles/{}/friends/?l=english",
            steam_id
        )
    }

    fn url_comments_page(&self, steam_id: &str, page: u32) -> String {
        format!(
            "https://steamcommunity.com/profiles/{}/allcomments/?l=english&ctp={}",
            steam_id, page
        )
    }

    pub async fn get_profile_content_by_steam_id(&self, steam_id: &str) -> Result<String> {
        {
            let cache = self.steam_profiles.read().await;
            if let Some(content) = cache.get(steam_id) {
                return Ok(content.clone());
            }
        }
        let content = self
            .request(&self.url_profile_by_steam_id(steam_id))
            .await?;
        self.steam_profiles
            .write()
            .await
            .insert(steam_id.to_string(), content.clone());
        Ok(content)
    }

    async fn get_profile_content_by_custom_id(&self, custom_id: &str) -> Result<String> {
        {
            let trans = self.custom_id_translation.read().await;
            if let Some(steam_id) = trans.get(custom_id) {
                let cache = self.steam_profiles.read().await;
                if let Some(content) = cache.get(steam_id) {
                    return Ok(content.clone());
                }
            }
        }

        let content = self
            .request(&self.url_profile_by_custom_id(custom_id))
            .await?;
        let steam_id = Self::extract_steam_id(&content);
        if steam_id.is_empty() {
            return Err(TeamDetectorError::NotFound(format!(
                "Steam ID not found for custom ID: {}",
                custom_id
            )));
        }

        self.custom_id_translation
            .write()
            .await
            .insert(custom_id.to_string(), steam_id.clone());
        self.steam_profiles
            .write()
            .await
            .insert(steam_id, content.clone());
        Ok(content)
    }

    async fn get_friends_content_by_steam_id(&self, steam_id: &str) -> Result<String> {
        {
            let cache = self.steam_friends.read().await;
            if let Some(content) = cache.get(steam_id) {
                return Ok(content.clone());
            }
        }
        let content = self
            .request(&self.url_friends_by_steam_id(steam_id))
            .await?;
        self.steam_friends
            .write()
            .await
            .insert(steam_id.to_string(), content.clone());
        Ok(content)
    }

    fn extract_steam_id(content: &str) -> String {
        if let Some(caps) = STEAM_ID_REGEX.captures(content) {
            return caps.get(1).map_or("", |m| m.as_str()).to_string();
        }
        String::new()
    }

    fn extract_custom_id(content: &str) -> String {
        if let Some(caps) = CUSTOM_ID_REGEX.captures(content) {
            return caps.get(1).map_or("", |m| m.as_str()).to_string();
        }
        String::new()
    }

    pub async fn get_steam_id_by_custom_id(&self, custom_id: &str) -> Result<String> {
        {
            let trans = self.custom_id_translation.read().await;
            if let Some(steam_id) = trans.get(custom_id) {
                return Ok(steam_id.clone());
            }
        }
        let content = self.get_profile_content_by_custom_id(custom_id).await?;
        Ok(Self::extract_steam_id(&content))
    }

    /// Gets a custom ID for a Steam ID.
    ///
    /// # Errors
    /// Returns an error if the HTTP request fails.
    pub async fn get_custom_id_by_steam_id(&self, steam_id: &str) -> Result<String> {
        {
            let trans = self.custom_id_translation.read().await;
            for (k, v) in trans.iter() {
                if v == steam_id {
                    return Ok(k.clone());
                }
            }
        }
        let content = self.get_profile_content_by_steam_id(steam_id).await?;
        Ok(Self::extract_custom_id(&content))
    }

    /// Gets the profile name for a Steam ID.
    ///
    /// # Errors
    /// Returns an error if the HTTP request fails.
    pub async fn get_profile_name(&self, steam_id: &str) -> Result<String> {
        let content = self.get_profile_content_by_steam_id(steam_id).await?;
        let document = Html::parse_document(&content);

        let name = document
            .select(&SELECTOR_PERSONA_NAME)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .unwrap_or_default();

        Ok(name)
    }

    /// Gets the profile status for a Steam ID.
    ///
    /// # Errors
    /// Returns an error if the HTTP request fails.
    pub async fn get_profile_status(&self, steam_id: &str) -> Result<String> {
        let content = self.get_profile_content_by_steam_id(steam_id).await?;
        let document = Html::parse_document(&content);

        if let Some(el) = document.select(&SELECTOR_IN_GAME_NAME).next() {
            let in_game = el.text().collect::<String>().trim().to_string();
            if !in_game.is_empty() {
                return Ok(format!("In-Game: {in_game}"));
            }
        }

        if let Some(el) = document.select(&SELECTOR_IN_GAME_HEADER).next() {
            let header = el.text().collect::<String>().trim().to_string();
            if !header.is_empty() {
                return Ok(header);
            }
        }

        Ok("Offline".to_string())
    }

    /// Gets the number of comments for a Steam ID.
    ///
    /// # Errors
    /// Returns an error if the HTTP request fails.
    pub async fn get_number_of_comments(&self, steam_id: &str) -> Result<u32> {
        let content = self.get_profile_content_by_steam_id(steam_id).await?;
        let document = Html::parse_document(&content);

        if let Some(el) = document.select(&SELECTOR_COMMENT_SPAN).next() {
            let text = el.text().collect::<String>();
            let num_str: String = text.chars().filter(char::is_ascii_digit).collect();
            if let Ok(num) = num_str.parse::<u32>() {
                return Ok(num);
            }
        }

        if let Some(caps) = COMMENTS_REGEX.captures(&content)
            && let Some(m) = caps.get(1)
            && let Ok(num) = m.as_str().parse::<u32>()
        {
            return Ok(num);
        }

        Ok(0)
    }

    /// Fetches a player's friends list from their Steam profile page.
    ///
    /// # Errors
    /// Returns an error if the HTTP request fails or parsing fails.
    pub async fn get_friends(&self, steam_id: &str) -> Result<Vec<Player>> {
        let content = self.get_friends_content_by_steam_id(steam_id).await?;
        let mut friends = Vec::new();
        let mut custom_ids_to_cache = Vec::new();

        {
            let document = Html::parse_document(&content);

            for block in document.select(&SELECTOR_FRIEND_BLOCK) {
                let friend_steam_id = block.value().attr("data-steamid").unwrap_or("").to_string();

                let mut href = String::new();
                if let Some(link) = block.select(&SELECTOR_FRIEND_LINK).next() {
                    href = link.value().attr("href").unwrap_or("").to_string();
                }

                let mut custom_id = None;
                if let Some(caps) = FRIEND_HREF_REGEX.captures(&href)
                    && let Some(m) = caps.get(1)
                {
                    custom_id = Some(m.as_str().to_string());
                }

                let mut name = String::new();
                if let Some(content_div) = block.select(&SELECTOR_FRIEND_CONTENT).next() {
                    let text: String = content_div
                        .children()
                        .filter_map(|node| node.value().as_text().map(|t| t.text.to_string()))
                        .collect();
                    name = text.trim().to_string();
                }

                if !friend_steam_id.is_empty() {
                    if let Some(cid) = &custom_id {
                        custom_ids_to_cache.push((cid.clone(), friend_steam_id.clone()));
                    }

                    friends.push(Player {
                        steam_id: Some(friend_steam_id),
                        custom_id,
                        name,
                        status: None,
                        is_on_server: None,
                        source_type: Some("friends".to_string()),
                    });
                }
            }
        }

        if !custom_ids_to_cache.is_empty() {
            let mut trans = self.custom_id_translation.write().await;
            for (cid, sid) in custom_ids_to_cache {
                trans.entry(cid).or_insert(sid);
            }
        }

        Ok(friends)
    }

    /// Fetches authors from a specific comments page.
    ///
    /// # Errors
    /// Returns an error if the HTTP request fails or parsing fails.
    pub async fn get_comments_page_authors(
        &self,
        steam_id: &str,
        page: u32,
    ) -> Result<(u32, Vec<Player>)> {
        let url = self.url_comments_page(steam_id, page);
        let content = self.request(&url).await?;

        let document = Html::parse_document(&content);

        let mut authors = Vec::new();
        let mut total_read = 0;

        for link in document.select(&SELECTOR_AUTHOR_LINK) {
            total_read += 1;
            let href = link.value().attr("href").unwrap_or("").to_string();

            let mut name = String::new();
            if let Some(bdi) = link.select(&SELECTOR_BDI).next() {
                name = bdi.text().collect::<String>().trim().to_string();
            }

            let mut author_steam_id = None;
            let mut author_custom_id = None;

            if let Some(caps) = COMMENT_STEAM_ID_REGEX.captures(&href) {
                if let Some(m) = caps.get(1) {
                    author_steam_id = Some(m.as_str().to_string());
                }
            } else if let Some(caps) = FRIEND_HREF_REGEX.captures(&href)
                && let Some(m) = caps.get(1)
            {
                author_custom_id = Some(m.as_str().to_string());
            }

            let duplicate = authors.iter().any(|a: &Player| {
                if let Some(sid) = &author_steam_id
                    && a.steam_id.as_ref() == Some(sid)
                {
                    return true;
                }
                if let Some(cid) = &author_custom_id
                    && a.custom_id.as_ref() == Some(cid)
                {
                    return true;
                }
                false
            });

            if !duplicate {
                authors.push(Player {
                    steam_id: author_steam_id,
                    custom_id: author_custom_id,
                    name,
                    status: None,
                    is_on_server: None,
                    source_type: Some("comments".to_string()),
                });
            }
        }

        Ok((total_read, authors))
    }
}
