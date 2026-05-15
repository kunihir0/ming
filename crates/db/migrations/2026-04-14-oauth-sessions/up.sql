CREATE TABLE users (
    discord_id TEXT PRIMARY KEY NOT NULL,
    username TEXT NOT NULL,
    avatar TEXT,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP NOT NULL
);

CREATE TABLE sessions (
    token TEXT PRIMARY KEY NOT NULL,
    discord_id TEXT NOT NULL,
    expires_at DATETIME NOT NULL,
    FOREIGN KEY (discord_id) REFERENCES users(discord_id) ON DELETE CASCADE
);