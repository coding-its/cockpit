-- Offene Posten: Bis jetzt gab es nur eine monatliche Auswertung, aber keine
-- Möglichkeit festzuhalten, welche Zeiten schon in einer Rechnung gelandet
-- sind. `billed_at` setzt genau diese Marke; NULL heißt: noch offen.
--
-- Bewusst ein Zeitstempel statt eines booleschen Feldes: So ist im Audit-Log
-- und in der Ablage nachvollziehbar, wann abgerechnet wurde. Ältere Einträge
-- bleiben offen – der erste Lauf der Aufstellung holt sie also nach.

ALTER TABLE time_entries ADD COLUMN billed_at TEXT;

-- Die Aufstellung sucht immer „offene Zeiten dieses Kunden" bzw. dieses
-- Web-Kunden; ohne Index wächst der Aufwand mit der Zahl der Zeiteinträge.
CREATE INDEX IF NOT EXISTS idx_time_customer_open
    ON time_entries (customer_id, billed_at);
CREATE INDEX IF NOT EXISTS idx_time_web_client_open
    ON time_entries (web_client_id, billed_at);
