-- Leistungsarten. Details siehe migrations_pg/0006_activity_kinds.sql.
CREATE TABLE IF NOT EXISTS activity_kinds (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    position INTEGER NOT NULL DEFAULT 0,
    active INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_activity_kinds_name ON activity_kinds (lower(name));

ALTER TABLE teamviewer_sessions ADD COLUMN activity TEXT;
