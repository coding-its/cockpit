-- WordPress-Kunden sind ein eigener Zweig, unabhaengig von NinjaOne-Kunden.
-- Ausserdem: Zeiteintraege koennen zu einem Web-Kunden gehoeren und tragen
-- einen Kommentar (Feld note, in der Oberflaeche als "Kommentar" gefuehrt).

CREATE TABLE web_clients (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    short_code TEXT NOT NULL,
    contact_name TEXT,
    contact_email TEXT,
    phone TEXT,
    website_url TEXT,
    notes TEXT,
    hourly_rate REAL NOT NULL DEFAULT 0,
    active INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL
);

CREATE INDEX idx_web_clients_active ON web_clients(active);

-- sites: Web-Kunde statt NinjaOne-Kunde. customer_id wird optional,
-- damit bestehende Zeilen und ein spaeterer Umbau nicht brechen.
CREATE TABLE sites_new (
    id TEXT PRIMARY KEY,
    customer_id TEXT REFERENCES customers(id) ON DELETE CASCADE,
    web_client_id TEXT REFERENCES web_clients(id) ON DELETE CASCADE,
    url TEXT NOT NULL,
    label TEXT,
    environment TEXT NOT NULL DEFAULT 'prod',
    agent_mode TEXT NOT NULL DEFAULT 'rest',
    wp_version TEXT,
    php_version TEXT,
    plugin_updates INTEGER NOT NULL DEFAULT 0,
    core_update INTEGER NOT NULL DEFAULT 0,
    status TEXT NOT NULL DEFAULT 'healthy',
    last_checked TEXT,
    backup_age_hours INTEGER,
    notes TEXT,
    agent_user TEXT,
    agent_password TEXT,
    agent_secret TEXT,
    last_version_sync TEXT,
    backup_confirmed INTEGER NOT NULL DEFAULT 0,
    backup_confirmed_at TEXT,
    backup_reference TEXT,
    login_url TEXT,
    login_user TEXT,
    login_password TEXT,
    login_note TEXT
);

INSERT INTO sites_new (
    id, customer_id, web_client_id, url, label, environment, agent_mode,
    wp_version, php_version, plugin_updates, core_update, status, last_checked,
    backup_age_hours, notes, agent_user, agent_password, agent_secret,
    last_version_sync, backup_confirmed, backup_confirmed_at, backup_reference
)
SELECT
    id, customer_id, NULL, url, label, environment, agent_mode,
    wp_version, php_version, plugin_updates, core_update, status, last_checked,
    backup_age_hours, notes, agent_user, agent_password, agent_secret,
    last_version_sync, backup_confirmed, backup_confirmed_at, backup_reference
FROM sites;

DROP TABLE sites;
ALTER TABLE sites_new RENAME TO sites;

CREATE INDEX idx_sites_customer ON sites(customer_id);
CREATE INDEX idx_sites_web_client ON sites(web_client_id);

-- time_entries: Zuordnung optional zu NinjaOne-Kunde ODER Web-Kunde,
-- ausserdem optional eine Site (fuer Detailnachweise).
CREATE TABLE time_entries_new (
    id TEXT PRIMARY KEY,
    customer_id TEXT REFERENCES customers(id) ON DELETE CASCADE,
    web_client_id TEXT REFERENCES web_clients(id) ON DELETE CASCADE,
    site_id TEXT REFERENCES sites(id) ON DELETE SET NULL,
    run_id TEXT,
    task_id TEXT,
    user_name TEXT NOT NULL,
    activity TEXT NOT NULL,
    note TEXT,
    started_at TEXT NOT NULL,
    ended_at TEXT,
    duration_seconds INTEGER NOT NULL DEFAULT 0,
    billable INTEGER NOT NULL DEFAULT 1,
    source TEXT NOT NULL DEFAULT 'timer',
    created_at TEXT NOT NULL
);

INSERT INTO time_entries_new (
    id, customer_id, web_client_id, site_id, run_id, task_id, user_name,
    activity, note, started_at, ended_at, duration_seconds, billable, source, created_at
)
SELECT
    id, customer_id, NULL, NULL, run_id, task_id, user_name,
    activity, note, started_at, ended_at, duration_seconds, billable, source, created_at
FROM time_entries;

DROP TABLE time_entries;
ALTER TABLE time_entries_new RENAME TO time_entries;

CREATE INDEX idx_time_customer ON time_entries(customer_id);
CREATE INDEX idx_time_web_client ON time_entries(web_client_id);
CREATE INDEX idx_time_started ON time_entries(started_at);

-- Aktionen auf einer Website (Health, Plugin-Update, Core-Update).
-- Ersetzt fuer Websites den Wartungslauf: eine Aktion, ein Nachweis, eine Zeit.
CREATE TABLE web_actions (
    id TEXT PRIMARY KEY,
    site_id TEXT NOT NULL REFERENCES sites(id) ON DELETE CASCADE,
    web_client_id TEXT NOT NULL REFERENCES web_clients(id) ON DELETE CASCADE,
    kind TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'queued',
    progress INTEGER NOT NULL DEFAULT 0,
    message TEXT,
    detail TEXT,
    before_version TEXT,
    after_version TEXT,
    duration_ms INTEGER,
    created_at TEXT NOT NULL,
    finished_at TEXT
);

CREATE INDEX idx_web_actions_site ON web_actions(site_id);
CREATE INDEX idx_web_actions_created ON web_actions(created_at);
