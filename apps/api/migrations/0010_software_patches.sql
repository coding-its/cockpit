-- Software-Patches neben den Betriebssystem-Patches.
--
-- NinjaOne führt beide getrennt (`/v2/queries/os-patches` und
-- `/v2/queries/software-patches`). Die Software-Seite ist deutlich größer –
-- im Test-Tenant rund 24.000 Zeilen, fast alles abgelehnt –, deshalb wird die
-- Art mitgespeichert und in der Oberfläche getrennt angezeigt.

ALTER TABLE device_patches ADD COLUMN patch_kind TEXT NOT NULL DEFAULT 'os';

CREATE INDEX IF NOT EXISTS idx_device_patches_kind ON device_patches(device_id, patch_kind);
