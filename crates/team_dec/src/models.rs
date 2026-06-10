use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct Player {
    pub steam_id: Option<String>,
    pub custom_id: Option<String>,
    pub name: String,
    pub status: Option<String>,
    pub is_on_server: Option<bool>,
    pub source_type: Option<String>, // "friends" or "comments"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphNode {
    pub id: String,
    pub label: String,
    pub steam_id: Option<String>,
    pub custom_id: Option<String>,
    pub status: Option<String>,
    pub is_on_server: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphEdge {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphData {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionData {
    pub name: String,
    pub custom_id: Option<String>,
    pub connections: Vec<Player>,
}

// ---------------------------------------------------------
// steamid.com Models
// ---------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SteamIdFriend {
    pub account_id: u32,
    pub steam_id64: String,
    pub persona_name: String,
    pub privacy_state: String,
    pub friend_since: String,
    pub member_since: String,
    pub depth: u32,
    pub friend_of: u32,
    pub bans: BansInfo,
    pub mutual_friends: Vec<u32>,
    pub total_friends: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BansInfo {
    pub community_banned: bool,
    pub vac_bans: u32,
    pub game_bans: u32,
    pub economy_ban: String,
}
