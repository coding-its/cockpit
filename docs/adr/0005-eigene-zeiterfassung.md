# ADR 0005 – Eigene Zeiterfassung mit neutralem Export

**Status:** entschieden · Oktober 2026

## Kontext

Die Abrechnung ist der eigentliche Nutzen des Cockpits. Gleichzeitig ist noch offen, ob DATEV, sevDesk oder Lexware Office das führende System wird. Ein direkter PSA-Sync würde das Cockpit von einem externen Produkt und dessen Datenmodell abhängig machen.

## Entscheidung

- Das Cockpit ist im PoC führend für Zeiten, Wartungsläufe und Leistungsnachweise.
- Zeiten werden pro Kunde, Lauf und Aufgabe erfasst. Quelle ist `timer`, `manual` oder `machine`.
- Maschinenzeit ist getrennt und standardmäßig nicht abrechenbar.
- Vertragslogik: Pauschale + Überhang über Inklusivstunden × Stundensatz. Kein Übertrag in Version 1.
- Export als neutraler CSV-Leistungsnachweis mit deutschem Format.
- sevDesk/Lexware-Adapter und DATEV-Feinabstimmung folgen in Phase 3.

## Konsequenzen

**Positiv:** Keine Abhängigkeit von einem PSA im PoC. Die Abrechnung bleibt nachvollziehbar und kann mit dem Steuerberater abgestimmt werden. Ein späterer Adapter muss nur den Export abholen, nicht die Erfassung ersetzen.

**Negativ:** Doppelte Erfassung, falls das Team bereits NinjaOne-Ticketing oder ein PSA nutzt. Die Vertragslogik ist bewusst einfach und bildet Sonderfälle (Übertrag, Mindestumsatz, Material) noch nicht ab.

**Alternativen:** Zeiten direkt in NinjaOne-Tickets schreiben. Das ist sinnvoll, wenn NinjaOne bereits das PSA ist – dann wird das Cockpit zum Frontend und NinjaOne bleibt führend. Diese Entscheidung ist bewusst offen und in Phase 3 revidierbar.
