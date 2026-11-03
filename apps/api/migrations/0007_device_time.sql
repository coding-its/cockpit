-- Zeiterfassung direkt auf einem Gerät.
-- Bisher hing eine Zeit an Kunde, Website oder Wartungslauf; jetzt auch an
-- einer einzelnen Maschine. Der Kunde wird dabei mitgeschrieben, damit die
-- Abrechnung weiterhin nach Kunde gruppiert.

ALTER TABLE time_entries ADD COLUMN device_id TEXT REFERENCES devices(id) ON DELETE SET NULL;

CREATE INDEX idx_time_device ON time_entries(device_id);
