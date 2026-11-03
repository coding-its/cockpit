-- Konkrete Aktion einer Aufgabe. Bisher wurde sie aus dem Titel geraten
-- ("scan" im Titel ⇒ Scan). Mit Software-Updates, Neustart und Wartungsmodus
-- reicht das nicht mehr – der Titel ist für Menschen, die Aktion für den Code.
--
-- action: patch_scan_os | patch_apply_os | patch_scan_software
--         | patch_apply_software | reboot_normal | reboot_forced
--         | maintenance_start | maintenance_stop
-- action_params: JSON mit Zusatzangaben (Begründung, Dauer, unterdrückte Features)
ALTER TABLE tasks ADD COLUMN action TEXT;
ALTER TABLE tasks ADD COLUMN action_params TEXT;

-- Bis wann in NinjaOne ein Wartungsfenster für dieses Gerät gesetzt ist.
-- NinjaOne hat keinen Lese-Endpunkt dafür; wir merken uns, was wir gesetzt haben.
ALTER TABLE devices ADD COLUMN maintenance_until TEXT;
