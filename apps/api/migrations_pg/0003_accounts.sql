-- Abrechnungskonten: eine Firma, eine Rechnung – auch wenn sie im Cockpit
-- über mehrere Quellen verteilt ist.
--
-- Anlass: „Autohaus Schmohl" heißt in NinjaOne so, in TeamViewer „AHSchmohl",
-- und dieselbe Firma kann zusätzlich als Web-Kunde geführt werden. Ohne
-- gemeinsamen Bezug erscheint sie im Bericht mehrfach, und die Rechnung
-- zerfällt in Bruchstücke.
--
-- Bewusst NICHT `short_code` als Schlüssel: Das Kürzel ist Anzeige (steht im
-- Bericht neben dem Namen) und ist nicht eindeutig (`IS` gibt es zweimal).
-- Ein Konto hat ein eigenes Kürzel, einen Namen und einen stabilen Verweis.
--
-- `account_id` ist absichtlich NULL-bar: Ohne Konto rechnet ein Datensatz wie
-- bisher für sich selbst. Nichts an bestehenden Daten ändert sich dadurch,
-- Konten können Kunde für Kunde gesetzt werden.

CREATE TABLE accounts (
    id TEXT PRIMARY KEY,
    -- Kürzel für die Anzeige, z. B. „AHS".
    code TEXT NOT NULL,
    name TEXT NOT NULL,
    active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TEXT NOT NULL
);

-- Kürzel sind innerhalb der Konten eindeutig – unabhängig von Groß- und
-- Kleinschreibung, damit „ahs" und „AHS" nicht zwei Konten werden.
CREATE UNIQUE INDEX idx_accounts_code ON accounts (lower(code));

ALTER TABLE customers ADD COLUMN account_id TEXT REFERENCES accounts(id) ON DELETE SET NULL;
ALTER TABLE web_clients ADD COLUMN account_id TEXT REFERENCES accounts(id) ON DELETE SET NULL;

CREATE INDEX IF NOT EXISTS idx_customers_account ON customers (account_id);
CREATE INDEX IF NOT EXISTS idx_web_clients_account ON web_clients (account_id);
