-- TeamViewer-Verbindungen nachbearbeiten.
--
-- Drei Fälle aus der Praxis:
--
-- 1. Die TeamViewer-Notiz ist eine Vorlage, kein Ergebnis. Oft kommt der
--    eigentliche Kommentar erst später („Anruf dazwischen", Kundenwunsch).
--    `note_edit` überschreibt sie, bleibt aber NULL, solange niemand etwas
--    ändert – so ist immer erkennbar, ob ein Mensch nachgearbeitet hat.
--
-- 2. Vergessene Logouts und unterbrochene Sitzungen: Die Zeitstempel stimmen,
--    die Verbindungsdauer ist aber nicht die Arbeitszeit. `duration_override_seconds`
--    ersetzt sie für die Übernahme; `duration_seconds` bleibt als Original
--    erhalten und wird im Nachweis mitgeführt.
--
-- 3. Doppelte Sitzungen (ein Arbeitsvorgang auf zwei Rechnern): Die zweite
--    wird `ignored` und taucht weder als offen noch im Import auf.

ALTER TABLE teamviewer_sessions ADD COLUMN note_edit TEXT;
ALTER TABLE teamviewer_sessions ADD COLUMN duration_override_seconds BIGINT;
ALTER TABLE teamviewer_sessions ADD COLUMN ignored BOOLEAN NOT NULL DEFAULT FALSE;
