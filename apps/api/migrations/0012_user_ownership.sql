-- Zeiterfassung pro Person. Bisher galt der Timer global: `active_timer`
-- nahm einfach den neuesten offenen Eintrag, egal wem er gehörte. Sobald zwei
-- Personen gleichzeitig arbeiten, stoppte der Start des einen den Timer des
-- anderen – und der Eintrag trug anschließend den falschen Namen.
--
-- `user_name` bleibt als Anzeige erhalten (auch nach Umbenennung lesbar),
-- `user_id` ist der stabile Bezug für Abfragen und Auswertungen.
ALTER TABLE time_entries ADD COLUMN user_id TEXT REFERENCES app_users(id) ON DELETE SET NULL;

-- Bestehende Einträge über den Anzeigenamen zuordnen. Das trifft nur, wenn
-- der Name exakt und eindeutig ist; alles andere bleibt NULL und wird in den
-- Auswertungen weiter über user_name geführt.
UPDATE time_entries
   SET user_id = (
        SELECT u.id FROM app_users u
         WHERE lower(trim(u.display_name)) = lower(trim(time_entries.user_name))
         LIMIT 1
   )
 WHERE user_id IS NULL;

CREATE INDEX IF NOT EXISTS idx_time_user ON time_entries(user_id);
CREATE INDEX IF NOT EXISTS idx_time_open ON time_entries(user_id, ended_at);
