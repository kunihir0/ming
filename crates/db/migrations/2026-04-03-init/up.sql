CREATE TABLE guild_configs (
    guild_id TEXT PRIMARY KEY NOT NULL,
    setup_mode TEXT NOT NULL DEFAULT 'Auto',
    manual_dashboard_channel_id TEXT,
    manual_chat_channel_id TEXT,
    manual_alerts_channel_id TEXT,
    in_game_prefix TEXT NOT NULL DEFAULT '@'
);

CREATE TABLE fcm_credentials (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    guild_id TEXT NOT NULL,
    gcm_android_id TEXT NOT NULL,
    gcm_security_token TEXT NOT NULL,
    steam_id TEXT NOT NULL,
    issued_date BIGINT NOT NULL,
    expire_date BIGINT NOT NULL,
    FOREIGN KEY (guild_id) REFERENCES guild_configs(guild_id)
);

CREATE TABLE paired_servers (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    fcm_credential_id INTEGER NOT NULL,
    server_ip TEXT NOT NULL,
    server_port INTEGER NOT NULL,
    player_token INTEGER NOT NULL,
    name TEXT NOT NULL,
    UNIQUE(server_ip, server_port, player_token),
    FOREIGN KEY (fcm_credential_id) REFERENCES fcm_credentials(id)
);

CREATE TABLE server_channels (
    server_id INTEGER PRIMARY KEY NOT NULL,
    category_id TEXT,
    dashboard_channel_id TEXT,
    chat_channel_id TEXT,
    alerts_channel_id TEXT,
    dashboard_message_id TEXT,
    FOREIGN KEY (server_id) REFERENCES paired_servers(id) ON DELETE CASCADE
);
