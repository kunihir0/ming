CREATE TABLE player_sessions (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    tracked_player_id INTEGER NOT NULL REFERENCES tracked_players(id) ON DELETE CASCADE,
    server_id INTEGER NOT NULL REFERENCES paired_servers(id) ON DELETE CASCADE,
    steam_id TEXT NOT NULL,
    joined_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    left_at TIMESTAMP,
    duration_secs INTEGER
);

CREATE INDEX idx_sessions_player ON player_sessions(tracked_player_id);
CREATE INDEX idx_sessions_server ON player_sessions(server_id);
CREATE INDEX idx_sessions_steam ON player_sessions(steam_id);
CREATE INDEX idx_sessions_joined ON player_sessions(joined_at);
