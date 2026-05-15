CREATE TABLE server_settings (
    server_id INTEGER PRIMARY KEY NOT NULL,
    in_game_prefix TEXT NOT NULL DEFAULT '!',
    bridge_rust_to_discord INTEGER NOT NULL DEFAULT 1,
    bridge_discord_to_rust INTEGER NOT NULL DEFAULT 1,
    command_cooldown INTEGER NOT NULL DEFAULT 0,
    chat_cooldown INTEGER NOT NULL DEFAULT 0,
    FOREIGN KEY (server_id) REFERENCES paired_servers(id) ON DELETE CASCADE
);
ALTER TABLE server_channels ADD COLUMN config_channel_id TEXT;
ALTER TABLE server_channels ADD COLUMN config_message_id TEXT;
