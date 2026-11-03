-- Fokus-Kunden: kleine Kunden bleiben verwaltet, tauchen aber nicht
-- in der Übersicht auf. Vorbelegung: ab 5 Geräten im Fokus.
ALTER TABLE customers ADD COLUMN focus INTEGER NOT NULL DEFAULT 1;

UPDATE customers
SET focus = CASE
    WHEN (SELECT COUNT(*) FROM devices d WHERE d.customer_id = customers.id) >= 5 THEN 1
    ELSE 0
END;

CREATE INDEX IF NOT EXISTS idx_customers_focus ON customers(focus);
