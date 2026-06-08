CREATE TABLE vending_subscriptions (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    discord_id TEXT,
    steam_id TEXT,
    server_id INTEGER NOT NULL,
    item_id INTEGER NOT NULL,
    item_name TEXT NOT NULL,
    max_price INTEGER,
    FOREIGN KEY(server_id) REFERENCES paired_servers(id) ON DELETE CASCADE
);
