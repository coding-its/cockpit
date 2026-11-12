-- Leistungsarten (Kategorien) für Zeiteinträge.
--
-- Bisher war `activity` freier Text: jeder tippt anders, dieselbe Leistung
-- heißt dann „Fernwartung", „Remote" oder „remote support". Für die
-- Auswertung ist das wertlos.
--
-- Die Liste ist pflegbar, nicht fest eingebaut – jede Firma teilt anders ein.
-- `time_entries.activity` bleibt Text; der Eintrag speichert den Namen zur
-- Zeit der Erfassung, damit spätere Umbenennungen alte Nachweise nicht
-- verändern.

CREATE TABLE activity_kinds (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    position BIGINT NOT NULL DEFAULT 0,
    active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TEXT NOT NULL
);

CREATE UNIQUE INDEX idx_activity_kinds_name ON activity_kinds (lower(name));

-- Ausgangsbestand. Frei anpassbar: umbenennen, ergänzen, stilllegen.
INSERT INTO activity_kinds (id, name, position, active, created_at) VALUES
    ('ak-01', 'Remote Support', 10, true, '2026-11-12T00:00:00'),
    ('ak-02', 'Remote Support – außerhalb GZ', 20, true, '2026-11-12T00:00:00'),
    ('ak-03', 'Vor-Ort-Service', 30, true, '2026-11-12T00:00:00'),
    ('ak-04', 'Vor-Ort-Service – außerhalb GZ', 40, true, '2026-11-12T00:00:00'),
    ('ak-05', 'Systemadministration & Datenpflege', 50, true, '2026-11-12T00:00:00'),
    ('ak-06', 'System-Monitoring & Alert-Handling', 60, true, '2026-11-12T00:00:00'),
    ('ak-07', 'Backup-Überwachung & Validierung', 70, true, '2026-11-12T00:00:00'),
    ('ak-08', 'Endpoint-Hardware-Service', 80, true, '2026-11-12T00:00:00'),
    ('ak-09', 'Technische Analyse & Consulting', 90, true, '2026-11-12T00:00:00'),
    ('ak-10', 'Projektplanung & Konzeption', 100, true, '2026-11-12T00:00:00'),
    ('ak-11', 'Kundenkommunikation (E-Mail)', 110, true, '2026-11-12T00:00:00'),
    ('ak-12', 'Kundenkommunikation (Telefon)', 120, true, '2026-11-12T00:00:00'),
    ('ak-13', 'Buchhaltung', 130, true, '2026-11-12T00:00:00'),
    ('ak-14', 'Webdesign', 140, true, '2026-11-12T00:00:00');

-- Leistungsart je TeamViewer-Verbindung. `NULL` heißt: die Vorgabe des
-- Imports („Remote Support") gilt.
ALTER TABLE teamviewer_sessions ADD COLUMN activity TEXT;
