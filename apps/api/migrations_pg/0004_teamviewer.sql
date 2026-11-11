-- TeamViewer: Verbindungen, die im TeamViewer-Konto protokolliert sind.
--
-- TeamViewer führt einen eigenen Kundenstamm (Gruppen). Diese Tabelle bildet
-- die Brücke in den vorhandenen Bestand: Eine Gruppe wird einem Kunden
-- zugeordnet, danach lassen sich ihre Verbindungen als Zeiteinträge
-- übernehmen – und laufen über dessen Abrechnungskonto in die Rechnung.
--
-- Bewusst KEIN eigener Kundenstamm: Sonst stünde dieselbe Firma dreimal im
-- System und die Auswertung pro Kunde zerfiele.

CREATE TABLE teamviewer_groups (
    -- TeamViewer-ID der Gruppe („g12345678").
    group_id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    -- Zuordnung in den vorhandenen Bestand. Genau eine der beiden Spalten
    -- ist gesetzt; beide NULL heißt: noch nicht zugeordnet.
    customer_id TEXT REFERENCES customers(id) ON DELETE SET NULL,
    web_client_id TEXT REFERENCES web_clients(id) ON DELETE SET NULL,
    -- Bewusst ausgeblendet (interne Geräte, Testgruppe).
    ignored BOOLEAN NOT NULL DEFAULT FALSE,
    last_seen TEXT,
    updated_at TEXT NOT NULL
);

CREATE TABLE teamviewer_sessions (
    -- TeamViewer-ID des Verbindungssatzes.
    id TEXT PRIMARY KEY,
    group_id TEXT,
    group_name TEXT,
    device_id TEXT,
    device_name TEXT,
    user_name TEXT,
    started_at TEXT NOT NULL,
    ended_at TEXT,
    duration_seconds BIGINT NOT NULL,
    -- Betrag, den TeamViewer selbst errechnet hat (Minuten × Satz).
    -- Nur als Vergleich, gerechnet wird im Cockpit.
    fee DOUBLE PRECISION NOT NULL DEFAULT 0,
    notes TEXT,
    -- Gesetzt, sobald daraus ein Zeiteintrag wurde.
    imported_entry_id TEXT,
    synced_at TEXT NOT NULL
);

CREATE INDEX idx_tv_sessions_month ON teamviewer_sessions (started_at);
CREATE INDEX idx_tv_sessions_group ON teamviewer_sessions (group_id, imported_entry_id);
