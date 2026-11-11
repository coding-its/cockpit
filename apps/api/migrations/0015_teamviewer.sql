-- TeamViewer-Gruppen und Verbindungen. Details siehe migrations_pg/0004_teamviewer.sql.
CREATE TABLE IF NOT EXISTS teamviewer_groups (
    group_id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    customer_id TEXT REFERENCES customers(id) ON DELETE SET NULL,
    web_client_id TEXT REFERENCES web_clients(id) ON DELETE SET NULL,
    ignored INTEGER NOT NULL DEFAULT 0,
    last_seen TEXT,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS teamviewer_sessions (
    id TEXT PRIMARY KEY,
    group_id TEXT,
    group_name TEXT,
    device_id TEXT,
    device_name TEXT,
    user_name TEXT,
    started_at TEXT NOT NULL,
    ended_at TEXT,
    duration_seconds INTEGER NOT NULL,
    fee DOUBLE PRECISION NOT NULL DEFAULT 0,
    notes TEXT,
    imported_entry_id TEXT,
    synced_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_tv_sessions_month ON teamviewer_sessions (started_at);
CREATE INDEX IF NOT EXISTS idx_tv_sessions_group ON teamviewer_sessions (group_id, imported_entry_id);
