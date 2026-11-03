# ADR 0003 – NinjaOne als Ausführungsquelle, Mock-first

**Status:** entschieden · Oktober 2026

## Kontext

NinjaOne bietet eine breite Public API, aber kein Entwickler-Tenant steht im PoC zur Verfügung. Gleichzeitig sind zwei Fragen offen, die erst mit einem echten Tenant beantwortet werden können: Funktioniert `script/run` mit einem Machine-Token, und ist der eine Webhook-Slot frei?

## Entscheidung

- NinjaOne bleibt die Quelle für Organisationen, Geräte, Patch-Status und Ausführungsjobs. Das Cockpit speichert nur Referenzen und den für die Wartung nötigen Cache.
- Der Client wird gegen die dokumentierten Endpunkte implementiert, inklusive OAuth, Token-Cache, Retry und Backoff.
- Ein Mock-Modus mit realistischen Fixtures ist Standard, solange keine Credentials gesetzt sind. Die gesamte Oberfläche und der Workflow sind damit ohne Tenant testbar.
- Live-Aufrufe werden erst in Phase 1 produktiv genutzt; der PoC bestätigt nur den Eingang asynchroner Jobs, nicht deren Abschluss.
- Keine Nachbildung von NinjaOne-Policies, Patch-Freigaben oder Remote Control.

## Konsequenzen

**Positiv:** Entwicklungsgeschwindigkeit ohne Zugangsdaten, klare Trennung zwischen Orchestrierung und Ausführung, geringes Risiko, versehentlich Produktionssysteme zu verändern.

**Negativ:** Der Mock kann API-Eigenheiten nicht abbilden. Die zwei offenen Fragen bleiben bis zum Live-Test. Der Client muss gegen die tenant-spezifische OpenAPI-Spezifikation nachgezogen werden.

**Alternativen:** Ein direkter Live-Start wäre schneller „echt“, aber ohne Tenant nicht möglich und ohne Audit- und Freigabekonzept riskant. Ein vollständiger RMM-Nachbau ist ausgeschlossen.
