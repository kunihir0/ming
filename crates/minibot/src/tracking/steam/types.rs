use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfileVisibility {
    Public,
    FriendsOnly,
    Private,
    NotSetup,
    Undetermined,
}

#[derive(Debug, Clone, Default)]
pub struct BanStatus {
    pub is_vac_banned: bool,
    pub is_community_banned: bool,
    pub is_game_banned: bool,
    pub game_ban_count: u32,
    pub days_since_last_ban: u32,
}

#[derive(Debug, Clone)]
pub struct SteamProfile {
    pub steam_id64: String,
    pub vanity_id: Option<String>,
    pub persona_name: String,
    pub real_name: Option<String>,
    pub visibility: ProfileVisibility,
    pub is_game_details_private: bool,
    pub avatar_url: Option<String>,
    pub level: u32,
    pub location: Option<String>,
    pub member_since: Option<DateTime<Utc>>,
    pub bans: BanStatus,
    pub summary: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SteamFriend {
    pub steam_id64: String,
    pub persona_name: String,
    pub friends_since: Option<DateTime<Utc>>,
}
