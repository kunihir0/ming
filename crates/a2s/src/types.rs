use thiserror::Error;

pub type Result<T, E = A2sError> = std::result::Result<T, E>;

#[derive(Debug, Error)]
pub enum A2sError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("Timeout error: {0}")]
    Timeout(#[from] tokio::time::error::Elapsed),
    
    #[error("Invalid packet format: {0}")]
    InvalidPacket(String),
}

#[derive(Debug, Clone)]
pub struct ServerInfo {
    pub protocol: u8,
    pub name: String,
    pub map: String,
    pub folder: String,
    pub game: String,
    pub app_id: u16,
    pub players: u8,
    pub max_players: u8,
    pub bots: u8,
    pub server_type: char,
    pub environment: char,
    pub visibility: u8,
    pub vac: u8,
    pub version: String,
    pub extra_data_flag: Option<u8>,
    pub real_players: Option<u16>,
    pub real_max_players: Option<u16>,
    pub keywords: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Player {
    pub index: u8,
    pub name: String,
    pub score: i32,
    pub duration: f32,
}
