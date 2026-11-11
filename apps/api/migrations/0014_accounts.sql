-- Abrechnungskonten. Details siehe migrations_pg/0003_accounts.sql.
CREATE TABLE IF NOT EXISTS accounts (
    id TEXT PRIMARY KEY,
    code TEXT NOT NULL,
    name TEXT NOT NULL,
    active INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_accounts_code ON accounts (lower(code));

ALTER TABLE customers ADD COLUMN account_id TEXT REFERENCES accounts(id) ON DELETE SET NULL;
ALTER TABLE web_clients ADD COLUMN account_id TEXT REFERENCES accounts(id) ON DELETE SET NULL;

CREATE INDEX IF NOT EXISTS idx_customers_account ON customers (account_id);
CREATE INDEX IF NOT EXISTS idx_web_clients_account ON web_clients (account_id);
