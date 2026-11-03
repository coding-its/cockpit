-- TeamViewer-ID je Gerät.
-- Der Aufruf läuft über das von TeamViewer registrierte URL-Schema
-- teamviewer10://control?device=<ID> – auf dem Client also ein Klick, ohne
-- Umweg über die NinjaOne-Oberfläche. Die ID wird von Hand gepflegt, weil
-- NinjaOne sie nicht mitliefert.

ALTER TABLE devices ADD COLUMN teamviewer_id TEXT;
