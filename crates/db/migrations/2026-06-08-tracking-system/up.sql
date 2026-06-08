-- Represents a group or clan
CREATE TABLE track_groups (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    server_id INTEGER NOT NULL,
    name TEXT NOT NULL,
    color TEXT,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY(server_id) REFERENCES paired_servers(id) ON DELETE CASCADE
);

-- Represents an individual tracked player
CREATE TABLE tracked_players (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    group_id INTEGER,
    server_id INTEGER NOT NULL,
    steam_id TEXT NOT NULL,
    bm_player_id TEXT,
    last_known_name TEXT,
    last_known_server_id TEXT,
    is_online INTEGER NOT NULL DEFAULT 0,
    last_seen DATETIME,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(server_id, steam_id),
    FOREIGN KEY(group_id) REFERENCES track_groups(id) ON DELETE SET NULL,
    FOREIGN KEY(server_id) REFERENCES paired_servers(id) ON DELETE CASCADE
);

-- Keeps a historical log of player names
CREATE TABLE player_name_history (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    tracked_player_id INTEGER NOT NULL,
    name TEXT NOT NULL,
    seen_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY(tracked_player_id) REFERENCES tracked_players(id) ON DELETE CASCADE
);

-- Configures where tracking notifications should be routed
CREATE TABLE track_notifications_config (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    server_id INTEGER NOT NULL UNIQUE,
    discord_channel_id TEXT,
    dashboard_message_id TEXT,
    in_game_alerts INTEGER NOT NULL DEFAULT 0,
    alert_on_join INTEGER NOT NULL DEFAULT 1,
    alert_on_leave INTEGER NOT NULL DEFAULT 1,
    alert_on_name_change INTEGER NOT NULL DEFAULT 1,
    FOREIGN KEY(server_id) REFERENCES paired_servers(id) ON DELETE CASCADE
);
