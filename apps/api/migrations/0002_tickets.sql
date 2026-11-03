-- Einsätze (Ad-hoc-Vorgänge) ergänzen das Wartungsmodell.
-- Ein Einsatz ist ein Run mit kind = 'ticket': gleiche Checkliste,
-- gleiche Zeiterfassung, gleiche Nachweise – nur ein anderer Einstieg.

ALTER TABLE runs ADD COLUMN kind TEXT NOT NULL DEFAULT 'maintenance';
ALTER TABLE runs ADD COLUMN priority TEXT NOT NULL DEFAULT 'normal';
ALTER TABLE runs ADD COLUMN description TEXT;
ALTER TABLE runs ADD COLUMN channel TEXT;
ALTER TABLE runs ADD COLUMN due_at TEXT;

CREATE INDEX IF NOT EXISTS idx_runs_kind ON runs(kind);
CREATE INDEX IF NOT EXISTS idx_runs_status ON runs(status);
