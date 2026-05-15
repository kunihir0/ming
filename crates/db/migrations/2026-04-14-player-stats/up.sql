CREATE TABLE player_stats (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    server_id INTEGER NOT NULL,
    steam_id TEXT NOT NULL,
    event_type TEXT NOT NULL,
    x REAL NOT NULL,
    y REAL NOT NULL,
    timestamp DATETIME DEFAULT CURRENT_TIMESTAMP NOT NULL
);

CREATE INDEX idx_player_stats_server_id ON player_stats (server_id);
CREATE INDEX idx_player_stats_steam_id ON player_stats (steam_id);
