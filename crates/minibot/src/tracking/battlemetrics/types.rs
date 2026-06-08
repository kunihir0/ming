#[derive(Debug, Clone, PartialEq)]
pub struct BmPlayer {
    pub bm_id: String,
    pub current_name: String,
    pub is_online: bool,
    pub current_server_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BmServerPlayerList {
    pub server_id: String,
    pub players: Vec<BmServerPlayer>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BmServerPlayer {
    pub bm_id: String,
    pub name: String,
}
