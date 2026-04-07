ALTER TABLE guild_configs ADD COLUMN management_channel_id TEXT;
CREATE TABLE pairing_requests (
    id TEXT PRIMARY KEY NOT NULL,
    guild_id TEXT NOT NULL,
    fcm_credential_id INTEGER NOT NULL,
    server_ip TEXT NOT NULL,
    server_port INTEGER NOT NULL,
    player_token INTEGER NOT NULL,
    name TEXT NOT NULL,
    FOREIGN KEY (guild_id) REFERENCES guild_configs(guild_id),
    FOREIGN KEY (fcm_credential_id) REFERENCES fcm_credentials(id)
);
