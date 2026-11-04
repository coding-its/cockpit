-- Offene Posten: Marke für „diese Zeit ist bereits abgerechnet".
-- NULL heißt offen. Details siehe migrations_pg/0002_billed.sql.
ALTER TABLE time_entries ADD COLUMN billed_at TEXT;

CREATE INDEX IF NOT EXISTS idx_time_customer_open
    ON time_entries (customer_id, billed_at);
CREATE INDEX IF NOT EXISTS idx_time_web_client_open
    ON time_entries (web_client_id, billed_at);
