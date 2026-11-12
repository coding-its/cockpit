-- TeamViewer-Verbindungen nachbearbeiten. Details siehe migrations_pg/0005_session_edits.sql.
ALTER TABLE teamviewer_sessions ADD COLUMN note_edit TEXT;
ALTER TABLE teamviewer_sessions ADD COLUMN duration_override_seconds INTEGER;
ALTER TABLE teamviewer_sessions ADD COLUMN ignored INTEGER NOT NULL DEFAULT 0;
