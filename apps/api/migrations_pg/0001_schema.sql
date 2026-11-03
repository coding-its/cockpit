-- Wartungscockpit – PostgreSQL-Schema
--
-- Ein einziges Schema für eine frische Datenbank. Die SQLite-Migrationen
-- (migrations/) bleiben unverändert liegen, weil sie den Weg der Altdaten
-- dokumentieren und das Importwerkzeug sie nicht braucht.
--
-- Zwei Übersetzungsregeln gegenüber SQLite:
--   * Zähler und IDs sind BIGINT, damit Rusts i64 ohne Umrechnung passt
--     (PostgreSQL liefert für INTEGER ein i32 und sqlx würde den Zugriff
--     zur Laufzeit ablehnen).
--   * Wahrheitswerte sind BOOLEAN. In SQLite waren es INTEGER mit 0/1 –
--     Vergleiche wie `active = 1` gibt es in Postgres nicht mehr.
--   * Geld und Stunden sind DOUBLE PRECISION statt REAL, damit die
--     Nachkommastellen für Cent-Beträge reichen.

CREATE TABLE customers (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    short_code TEXT NOT NULL,
    ninja_org_id BIGINT,
    contact_name TEXT,
    contact_email TEXT,
    phone TEXT,
    notes TEXT,
    active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TEXT NOT NULL,
    focus BOOLEAN NOT NULL DEFAULT TRUE
);

CREATE TABLE devices (
    id TEXT PRIMARY KEY,
    customer_id TEXT NOT NULL REFERENCES customers(id) ON DELETE CASCADE,
    ninja_device_id BIGINT,
    hostname TEXT NOT NULL,
    os TEXT,
    device_type TEXT,
    last_seen TEXT,
    patch_status TEXT NOT NULL DEFAULT 'ok',
    antivirus_status TEXT NOT NULL DEFAULT 'ok',
    online BOOLEAN NOT NULL DEFAULT TRUE,
    teamviewer_id TEXT,
    patch_pending BIGINT,
    patch_rejected BIGINT,
    patch_checked_at TEXT,
    antivirus_product TEXT,
    antivirus_state TEXT,
    antivirus_definitions TEXT,
    antivirus_checked_at TEXT,
    maintenance_until TEXT
);

CREATE TABLE web_clients (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    short_code TEXT NOT NULL,
    contact_name TEXT,
    contact_email TEXT,
    phone TEXT,
    website_url TEXT,
    notes TEXT,
    hourly_rate DOUBLE PRECISION NOT NULL DEFAULT 0,
    active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TEXT NOT NULL
);

CREATE TABLE sites (
    id TEXT PRIMARY KEY,
    customer_id TEXT REFERENCES customers(id) ON DELETE CASCADE,
    web_client_id TEXT REFERENCES web_clients(id) ON DELETE CASCADE,
    url TEXT NOT NULL,
    label TEXT,
    environment TEXT NOT NULL DEFAULT 'prod',
    agent_mode TEXT NOT NULL DEFAULT 'rest',
    wp_version TEXT,
    php_version TEXT,
    plugin_updates BIGINT NOT NULL DEFAULT 0,
    core_update BOOLEAN NOT NULL DEFAULT FALSE,
    status TEXT NOT NULL DEFAULT 'healthy',
    last_checked TEXT,
    backup_age_hours BIGINT,
    notes TEXT,
    agent_user TEXT,
    agent_password TEXT,
    agent_secret TEXT,
    last_version_sync TEXT,
    backup_confirmed BOOLEAN NOT NULL DEFAULT FALSE,
    backup_confirmed_at TEXT,
    backup_reference TEXT,
    login_url TEXT,
    login_user TEXT,
    login_password TEXT,
    login_note TEXT
);

CREATE TABLE contracts (
    id TEXT PRIMARY KEY,
    customer_id TEXT NOT NULL REFERENCES customers(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    interval TEXT NOT NULL DEFAULT 'monthly',
    included_hours DOUBLE PRECISION NOT NULL DEFAULT 0,
    hourly_rate DOUBLE PRECISION NOT NULL DEFAULT 0,
    flat_fee DOUBLE PRECISION NOT NULL DEFAULT 0,
    billing_day BIGINT NOT NULL DEFAULT 1,
    active BOOLEAN NOT NULL DEFAULT TRUE
);

CREATE TABLE app_users (
    id TEXT PRIMARY KEY,
    username TEXT NOT NULL,
    display_name TEXT NOT NULL,
    password_hash TEXT NOT NULL,
    role TEXT NOT NULL DEFAULT 'technician',
    active BOOLEAN NOT NULL DEFAULT TRUE,
    must_change_password BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TEXT NOT NULL,
    last_login_at TEXT
);

CREATE TABLE runs (
    id TEXT PRIMARY KEY,
    customer_id TEXT NOT NULL REFERENCES customers(id) ON DELETE CASCADE,
    contract_id TEXT REFERENCES contracts(id),
    period TEXT NOT NULL,
    title TEXT NOT NULL,
    scheduled_for TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'planned',
    assignee TEXT,
    notes TEXT,
    created_at TEXT NOT NULL,
    completed_at TEXT,
    kind TEXT NOT NULL DEFAULT 'maintenance',
    priority TEXT NOT NULL DEFAULT 'normal',
    description TEXT,
    channel TEXT,
    due_at TEXT
);

CREATE TABLE tasks (
    id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    position BIGINT NOT NULL,
    title TEXT NOT NULL,
    description TEXT,
    kind TEXT NOT NULL,
    target_type TEXT,
    target_id TEXT,
    target_label TEXT,
    status TEXT NOT NULL DEFAULT 'open',
    risk TEXT NOT NULL DEFAULT 'low',
    requires_approval BOOLEAN NOT NULL DEFAULT FALSE,
    approved BOOLEAN NOT NULL DEFAULT FALSE,
    result_summary TEXT,
    started_at TEXT,
    finished_at TEXT,
    action TEXT,
    action_params TEXT
);

CREATE TABLE jobs (
    id TEXT PRIMARY KEY,
    task_id TEXT,
    kind TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'queued',
    progress BIGINT NOT NULL DEFAULT 0,
    message TEXT,
    payload TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE task_executions (
    id TEXT PRIMARY KEY,
    task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    job_id TEXT,
    status TEXT NOT NULL,
    request TEXT,
    response TEXT,
    error TEXT,
    duration_ms BIGINT,
    before_version TEXT,
    after_version TEXT,
    created_at TEXT NOT NULL
);

CREATE TABLE time_entries (
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
    duration_seconds BIGINT NOT NULL DEFAULT 0,
    billable BOOLEAN NOT NULL DEFAULT TRUE,
    source TEXT NOT NULL DEFAULT 'timer',
    created_at TEXT NOT NULL,
    device_id TEXT REFERENCES devices(id) ON DELETE SET NULL,
    user_id TEXT REFERENCES app_users(id) ON DELETE SET NULL
);

CREATE TABLE audit_events (
    id TEXT PRIMARY KEY,
    actor TEXT NOT NULL,
    action TEXT NOT NULL,
    entity_type TEXT,
    entity_id TEXT,
    detail TEXT,
    created_at TEXT NOT NULL
);

CREATE TABLE app_settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);


CREATE TABLE sessions (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES app_users(id) ON DELETE CASCADE,
    token_hash TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    last_seen_at TEXT NOT NULL,
    user_agent TEXT
);

CREATE TABLE device_patches (
    device_id TEXT NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
    patch_id TEXT NOT NULL,
    name TEXT NOT NULL,
    kb_number TEXT,
    severity TEXT,
    status TEXT NOT NULL,
    patch_type TEXT,
    observed_at TEXT NOT NULL,
    patch_kind TEXT NOT NULL DEFAULT 'os',
    PRIMARY KEY (device_id, patch_id)
);

CREATE TABLE web_actions (
    id TEXT PRIMARY KEY,
    site_id TEXT NOT NULL REFERENCES sites(id) ON DELETE CASCADE,
    web_client_id TEXT NOT NULL REFERENCES web_clients(id) ON DELETE CASCADE,
    kind TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'queued',
    progress BIGINT NOT NULL DEFAULT 0,
    message TEXT,
    detail TEXT,
    before_version TEXT,
    after_version TEXT,
    duration_ms BIGINT,
    created_at TEXT NOT NULL,
    finished_at TEXT
);

CREATE INDEX idx_devices_customer ON devices(customer_id);
CREATE INDEX idx_sites_customer ON sites(customer_id);
CREATE INDEX idx_runs_customer ON runs(customer_id);
CREATE INDEX idx_tasks_run ON tasks(run_id);
CREATE INDEX idx_time_customer ON time_entries(customer_id);
CREATE INDEX idx_time_web_client ON time_entries(web_client_id);
CREATE INDEX idx_time_started ON time_entries(started_at);
CREATE INDEX idx_time_user ON time_entries(user_id);
CREATE INDEX idx_time_open ON time_entries(user_id, ended_at);
CREATE INDEX idx_customers_focus ON customers(focus);
CREATE INDEX idx_audit_created ON audit_events(created_at);
CREATE INDEX idx_sessions_user ON sessions(user_id);

-- Anmeldung und Einladung sind unabhängig von Groß- und Kleinschreibung.
-- SQLite löste das mit COLLATE NOCASE, Postgres über einen Ausdrucksindex.
CREATE UNIQUE INDEX idx_app_users_username ON app_users (lower(username));
