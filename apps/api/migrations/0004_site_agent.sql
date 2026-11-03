-- Zugangsdaten für den WordPress-Agent pro Site.
-- ACHTUNG PoC: Klartext in der Datenbank. Phase 1 verschlüsselt diese Felder
-- mit einem Master-Key aus dem Secret Store.

ALTER TABLE sites ADD COLUMN agent_user TEXT;
ALTER TABLE sites ADD COLUMN agent_password TEXT;
ALTER TABLE sites ADD COLUMN agent_secret TEXT;
ALTER TABLE sites ADD COLUMN last_version_sync TEXT;

-- Bestätigung, dass vor einem Update ein verifiziertes Backup existiert.
-- Wird pro Site gesetzt und nach jedem Update zurückgesetzt.
ALTER TABLE sites ADD COLUMN backup_confirmed INTEGER NOT NULL DEFAULT 0;
ALTER TABLE sites ADD COLUMN backup_confirmed_at TEXT;
ALTER TABLE sites ADD COLUMN backup_reference TEXT;
