CREATE TABLE user_rustplus_credentials (
    discord_id TEXT PRIMARY KEY NOT NULL,
    gcm_android_id TEXT NOT NULL,
    gcm_security_token TEXT NOT NULL,
    expo_push_token TEXT NOT NULL,
    rustplus_auth_token TEXT NOT NULL,
    FOREIGN KEY (discord_id) REFERENCES users(discord_id) ON DELETE CASCADE
);