CREATE TABLE vending_transactions (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    server_id INTEGER NOT NULL,
    timestamp BIGINT NOT NULL,
    item_id INTEGER NOT NULL,
    item_name TEXT NOT NULL,
    currency_id INTEGER NOT NULL,
    currency_name TEXT NOT NULL,
    quantity INTEGER NOT NULL,
    cost_per_item INTEGER NOT NULL,
    amount_in_stock INTEGER NOT NULL,
    is_outlier INTEGER NOT NULL DEFAULT 0,
    FOREIGN KEY (server_id) REFERENCES paired_servers(id) ON DELETE CASCADE
);

CREATE INDEX idx_vending_transactions_server_id_item_id ON vending_transactions (server_id, item_id);
CREATE INDEX idx_vending_transactions_timestamp ON vending_transactions (timestamp);
