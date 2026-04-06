use crate::db::schema::{fcm_credentials, guild_configs, paired_servers, server_channels};
use diesel::prelude::*;

#[derive(Queryable, Selectable, Insertable, Identifiable, AsChangeset, Debug, Clone)]
#[diesel(table_name = guild_configs)]
#[diesel(primary_key(guild_id))]
pub struct GuildConfig {
    pub guild_id: String,
    pub setup_mode: String,
    pub manual_dashboard_channel_id: Option<String>,
    pub manual_chat_channel_id: Option<String>,
    pub manual_alerts_channel_id: Option<String>,
    pub in_game_prefix: String,
}

#[derive(Queryable, Selectable, Identifiable, Associations, Debug, Clone)]
#[diesel(belongs_to(GuildConfig, foreign_key = guild_id))]
#[diesel(table_name = fcm_credentials)]
pub struct FcmCredential {
    pub id: i32,
    pub guild_id: String,
    pub gcm_android_id: String,
    pub gcm_security_token: String,
    pub steam_id: String,
    pub issued_date: i64,
    pub expire_date: i64,
}

#[derive(Insertable, Debug, Clone)]
#[diesel(table_name = fcm_credentials)]
pub struct NewFcmCredential {
    pub guild_id: String,
    pub gcm_android_id: String,
    pub gcm_security_token: String,
    pub steam_id: String,
    pub issued_date: i64,
    pub expire_date: i64,
}

#[derive(Queryable, Selectable, Identifiable, Associations, Debug, Clone)]
#[diesel(belongs_to(FcmCredential))]
#[diesel(table_name = paired_servers)]
pub struct PairedServer {
    pub id: i32,
    pub fcm_credential_id: i32,
    pub server_ip: String,
    pub server_port: i32,
    pub player_token: i32,
    pub name: String,
}

#[derive(Insertable, Debug, Clone)]
#[diesel(table_name = paired_servers)]
pub struct NewPairedServer {
    pub fcm_credential_id: i32,
    pub server_ip: String,
    pub server_port: i32,
    pub player_token: i32,
    pub name: String,
}

#[derive(
    Queryable, Selectable, Insertable, Identifiable, AsChangeset, Associations, Debug, Clone,
)]
#[diesel(belongs_to(PairedServer, foreign_key = server_id))]
#[diesel(primary_key(server_id))]
#[diesel(table_name = server_channels)]
pub struct ServerChannel {
    pub server_id: i32,
    pub category_id: Option<String>,
    pub dashboard_channel_id: Option<String>,
    pub chat_channel_id: Option<String>,
    pub alerts_channel_id: Option<String>,
    pub dashboard_message_id: Option<String>,
}
