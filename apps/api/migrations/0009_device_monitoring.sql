-- Patch- und Antivirenstand aus NinjaOne.
--
-- Bisher standen beide Werte fest auf 'unknown', weil der Sync sie nie abfragte
-- (bewusst ehrlich, aber ohne Aussage). NinjaOne liefert die Daten über
-- /v2/queries/os-patches und /v2/queries/antivirus-status für den ganzen
-- Bestand in je einem Aufruf.
--
-- `patch_status` und `antivirus_status` bleiben die abgeleitete Ampel, die die
-- Oberfläche schon kennt. Die neuen Spalten tragen die Details für das
-- Geräte-Panel; `unknown` heißt weiterhin "keine Daten", nicht "alles gut".

ALTER TABLE devices ADD COLUMN patch_pending INTEGER;
ALTER TABLE devices ADD COLUMN patch_rejected INTEGER;
ALTER TABLE devices ADD COLUMN patch_checked_at TEXT;
ALTER TABLE devices ADD COLUMN antivirus_product TEXT;
ALTER TABLE devices ADD COLUMN antivirus_state TEXT;
ALTER TABLE devices ADD COLUMN antivirus_definitions TEXT;
ALTER TABLE devices ADD COLUMN antivirus_checked_at TEXT;

-- Offene und abgelehnte Patches je Gerät. Wird bei jedem Abgleich ersetzt,
-- damit nicht veraltete Einträge stehen bleiben.
CREATE TABLE IF NOT EXISTS device_patches (
    device_id TEXT NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
    patch_id TEXT NOT NULL,
    name TEXT NOT NULL,
    kb_number TEXT,
    severity TEXT,
    status TEXT NOT NULL,
    patch_type TEXT,
    observed_at TEXT NOT NULL,
    PRIMARY KEY (device_id, patch_id)
);

CREATE INDEX IF NOT EXISTS idx_device_patches_status ON device_patches(status);
