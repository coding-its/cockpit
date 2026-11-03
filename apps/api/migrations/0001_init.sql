CREATE TABLE IF NOT EXISTS customers (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    short_code TEXT NOT NULL,
    ninja_org_id INTEGER,
    contact_name TEXT,
    contact_email TEXT,
    phone TEXT,
    notes TEXT,
    active INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS devices (
    id TEXT PRIMARY KEY,
    customer_id TEXT NOT NULL REFERENCES customers(id) ON DELETE CASCADE,
    ninja_device_id INTEGER,
    hostname TEXT NOT NULL,
    os TEXT,
    device_type TEXT,
    last_seen TEXT,
    patch_status TEXT NOT NULL DEFAULT 'ok',
    antivirus_status TEXT NOT NULL DEFAULT 'ok',
    online INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE IF NOT EXISTS sites (
    id TEXT PRIMARY KEY,
    customer_id TEXT NOT NULL REFERENCES customers(id) ON DELETE CASCADE,
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
    notes TEXT
);

CREATE TABLE IF NOT EXISTS contracts (
    id TEXT PRIMARY KEY,
    customer_id TEXT NOT NULL REFERENCES customers(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    interval TEXT NOT NULL DEFAULT 'monthly',
    included_hours REAL NOT NULL DEFAULT 0,
    hourly_rate REAL NOT NULL DEFAULT 0,
    flat_fee REAL NOT NULL DEFAULT 0,
    billing_day INTEGER NOT NULL DEFAULT 1,
    active INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE IF NOT EXISTS runs (
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
    completed_at TEXT
);

CREATE TABLE IF NOT EXISTS tasks (
    id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    position INTEGER NOT NULL,
    title TEXT NOT NULL,
    description TEXT,
    kind TEXT NOT NULL,
    target_type TEXT,
    target_id TEXT,
    target_label TEXT,
    status TEXT NOT NULL DEFAULT 'open',
    risk TEXT NOT NULL DEFAULT 'low',
    requires_approval INTEGER NOT NULL DEFAULT 0,
    approved INTEGER NOT NULL DEFAULT 0,
    result_summary TEXT,
    started_at TEXT,
    finished_at TEXT
);

CREATE TABLE IF NOT EXISTS jobs (
    id TEXT PRIMARY KEY,
    task_id TEXT,
    kind TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'queued',
    progress INTEGER NOT NULL DEFAULT 0,
    message TEXT,
    payload TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS task_executions (
    id TEXT PRIMARY KEY,
    task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    job_id TEXT,
    status TEXT NOT NULL,
    request TEXT,
    response TEXT,
    error TEXT,
    duration_ms INTEGER,
    before_version TEXT,
    after_version TEXT,
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS time_entries (
    id TEXT PRIMARY KEY,
    customer_id TEXT NOT NULL REFERENCES customers(id) ON DELETE CASCADE,
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

CREATE TABLE IF NOT EXISTS audit_events (
    id TEXT PRIMARY KEY,
    actor TEXT NOT NULL,
    action TEXT NOT NULL,
    entity_type TEXT,
    entity_id TEXT,
    detail TEXT,
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS app_settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_devices_customer ON devices(customer_id);
CREATE INDEX IF NOT EXISTS idx_sites_customer ON sites(customer_id);
CREATE INDEX IF NOT EXISTS idx_runs_customer ON runs(customer_id);
CREATE INDEX IF NOT EXISTS idx_tasks_run ON tasks(run_id);
CREATE INDEX IF NOT EXISTS idx_time_customer ON time_entries(customer_id);
CREATE INDEX IF NOT EXISTS idx_time_started ON time_entries(started_at);
CREATE INDEX IF NOT EXISTS idx_audit_created ON audit_events(created_at);
