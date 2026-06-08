use crate::schema::{
    fcm_credentials, guild_configs, paired_servers, pairing_requests, player_stats,
    server_channels, server_settings, sessions, user_rustplus_credentials, users,
    vending_subscriptions,
};
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
    pub manual_cctv_channel_id: Option<String>,
    pub manual_ai_channel_id: Option<String>,
    pub in_game_prefix: String,
    pub management_channel_id: Option<String>,
}

#[derive(Queryable, Selectable, Insertable, Identifiable, Debug, Clone)]
#[diesel(table_name = pairing_requests)]
pub struct PairingRequest {
    pub id: String,
    pub guild_id: String,
    pub fcm_credential_id: i32,
    pub server_ip: String,
    pub server_port: i32,
    pub player_token: i32,
    pub name: String,
}

#[derive(Insertable, Debug, Clone)]
#[diesel(table_name = pairing_requests)]
pub struct NewPairingRequest {
    pub id: String,
    pub guild_id: String,
    pub fcm_credential_id: i32,
    pub server_ip: String,
    pub server_port: i32,
    pub player_token: i32,
    pub name: String,
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
    pub auto_reconnect: i32,
}

#[derive(Insertable, Debug, Clone)]
#[diesel(table_name = paired_servers)]
pub struct NewPairedServer {
    pub fcm_credential_id: i32,
    pub server_ip: String,
    pub server_port: i32,
    pub player_token: i32,
    pub name: String,
    pub auto_reconnect: i32,
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
    pub config_channel_id: Option<String>,
    pub config_message_id: Option<String>,
    pub ai_channel_id: Option<String>,
    pub cctv_channel_id: Option<String>,
    pub cctv_message_id: Option<String>,
}

#[derive(
    Queryable, Selectable, Insertable, Identifiable, AsChangeset, Associations, Debug, Clone,
)]
#[diesel(belongs_to(PairedServer, foreign_key = server_id))]
#[diesel(primary_key(server_id))]
#[diesel(table_name = server_settings)]
pub struct ServerSettings {
    pub server_id: i32,
    pub in_game_prefix: String,
    pub bridge_rust_to_discord: i32,
    pub bridge_discord_to_rust: i32,
    pub command_cooldown: i32,
    pub chat_cooldown: i32,
    pub events_cargo: i32,
    pub events_heli: i32,
    pub events_oilrig: i32,
    pub events_ch47: i32,
    pub events_vending: i32,
}

#[derive(Insertable, Debug, Clone)]
#[diesel(table_name = server_settings)]
pub struct NewServerSettings {
    pub server_id: i32,
    pub in_game_prefix: String,
    pub bridge_rust_to_discord: i32,
    pub bridge_discord_to_rust: i32,
    pub command_cooldown: i32,
    pub chat_cooldown: i32,
    pub events_cargo: i32,
    pub events_heli: i32,
    pub events_oilrig: i32,
    pub events_ch47: i32,
    pub events_vending: i32,
}

#[derive(Queryable, Selectable, Insertable, Identifiable, AsChangeset, Debug, Clone)]
#[diesel(table_name = player_stats)]
pub struct PlayerStat {
    pub id: i32,
    pub server_id: i32,
    pub steam_id: String,
    pub event_type: String,
    pub x: f32,
    pub y: f32,
    pub timestamp: chrono::NaiveDateTime,
}

#[derive(Insertable, Debug, Clone)]
#[diesel(table_name = player_stats)]
pub struct NewPlayerStat {
    pub server_id: i32,
    pub steam_id: String,
    pub event_type: String,
    pub x: f32,
    pub y: f32,
}

#[derive(Queryable, Selectable, Insertable, Identifiable, AsChangeset, Debug, Clone, serde::Serialize)]
#[diesel(table_name = users)]
#[diesel(primary_key(discord_id))]
pub struct User {
    pub discord_id: String,
    pub username: String,
    pub avatar: Option<String>,
    pub created_at: chrono::NaiveDateTime,
}

#[derive(Insertable, Debug, Clone)]
#[diesel(table_name = users)]
pub struct NewUser {
    pub discord_id: String,
    pub username: String,
    pub avatar: Option<String>,
}

#[derive(Queryable, Selectable, Insertable, Identifiable, AsChangeset, Debug, Clone)]
#[diesel(table_name = sessions)]
#[diesel(primary_key(token))]
pub struct Session {
    pub token: String,
    pub discord_id: String,
    pub expires_at: chrono::NaiveDateTime,
}

#[derive(Insertable, Debug, Clone)]
#[diesel(table_name = sessions)]
pub struct NewSession {
    pub token: String,
    pub discord_id: String,
    pub expires_at: chrono::NaiveDateTime,
}

#[derive(Queryable, Selectable, Insertable, Identifiable, AsChangeset, Debug, Clone, serde::Serialize)]
#[diesel(table_name = user_rustplus_credentials)]
#[diesel(primary_key(discord_id))]
pub struct UserRustplusCredential {
    pub discord_id: String,
    pub gcm_android_id: String,
    pub gcm_security_token: String,
    pub expo_push_token: String,
    pub rustplus_auth_token: String,
}

#[derive(Insertable, Debug, Clone)]
#[diesel(table_name = user_rustplus_credentials)]
pub struct NewUserRustplusCredential {
    pub discord_id: String,
    pub gcm_android_id: String,
    pub gcm_security_token: String,
    pub expo_push_token: String,
    pub rustplus_auth_token: String,
}

#[derive(Queryable, Selectable, Insertable, Identifiable, AsChangeset, Associations, Debug, Clone)]
#[diesel(belongs_to(PairedServer, foreign_key = server_id))]
#[diesel(table_name = vending_subscriptions)]
pub struct VendingSubscription {
    pub id: i32,
    pub discord_id: Option<String>,
    pub steam_id: Option<String>,
    pub server_id: i32,
    pub item_id: i32,
    pub item_name: String,
    pub max_price: Option<i32>,
}

#[derive(Insertable, Debug, Clone)]
#[diesel(table_name = vending_subscriptions)]
pub struct NewVendingSubscription {
    pub discord_id: Option<String>,
    pub steam_id: Option<String>,
    pub server_id: i32,
    pub item_id: i32,
    pub item_name: String,
    pub max_price: Option<i32>,
}
