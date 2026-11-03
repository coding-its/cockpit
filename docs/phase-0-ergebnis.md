# Phase 0 – Ergebnisbericht

Stand: 26. Oktober 2026

## 1. Ergebnis auf einen Blick

Der Phase-0-Prototyp ist lauffähig und deckt den kompletten Wartungszyklus im Mock-Modus ab:

Kunde auswählen → Wartungslauf öffnen → Aufgabe freigeben/ausführen → Fortschritt und Ergebnis sehen → Timer starten/stoppen → Zeit und Abrechnung prüfen → CSV exportieren.

Die NinjaOne-Integration ist als echter Client mit OAuth, Retry und Job-Methoden implementiert, läuft aber ohne Tenant im Mock-Modus. Der WordPress-Agent liegt als Mu-Plugin vor und ist syntaktisch geprüft; ein Lauf gegen eine echte WordPress-Instanz war in dieser Umgebung nicht möglich.

**Empfehlung:** Phase 1 erst starten, wenn die drei Live-Fragen beantwortet sind: NinjaOne-API-Zugang, `script/run` mit Machine-Token und WordPress-Agent gegen eine Staging-Site.

## 2. Umfang

| Bereich | Umsetzung |
| --- | --- |
| Backend | Rust/Axum, SQLx, 3.836 Zeilen |
| Frontend | Vue 3/TypeScript/CSS, 5.561 Zeilen |
| WordPress-Agent | Mu-Plugin, 710 Zeilen |
| Dokumentation | Konzept, API-Referenz, 5 ADRs, Setup, 846 Zeilen |
| Demo-Daten | 4 Kunden, 13 Geräte, 5 WordPress-Sites, 4 Wartungsläufe, 19 Aufgaben, 10 Zeiteinträge |

## 3. Verifiziert

### Build und Tests

| Prüfung | Ergebnis |
| --- | --- |
| `cargo check` | erfolgreich, keine Warnungen nach Cleanup |
| `cargo test` | 4 von 4 Tests grün |
| `vue-tsc --noEmit` | ohne Typfehler |
| `vite build` | erfolgreich, 1.614 Module, Hauptbundle ~50 kB gzip |
| PHP-Syntax via `php-parser` | erfolgreich |
| `node --check` Testclient | erfolgreich |

Die vier Rust-Tests prüfen:

- Mock-Client liefert vier Organisationen.
- Mock-Client liefert Geräte pro Organisation.
- Mock-Jobs sind als Mock markiert.
- CSV-Export nutzt Semikolon, deutsches Datum und deutsches Dezimalkomma.

### API-Smoke-Tests

| Aufruf | Ergebnis |
| --- | --- |
| `GET /api/v1/health` | `ok`, Datenbank `ok`, NinjaOne `mock` |
| `GET /api/v1/dashboard` | 4 Kunden, 5 Sites, 3 offene Wartungen, 11,5 abrechenbare Stunden, 1.330,00 € |
| `GET /api/v1/customers` | 4 Kunden mit korrekten Aggregaten |
| `GET /api/v1/runs` | 4 Läufe: geplant, läuft, blockiert, abgeschlossen |
| `POST /api/v1/tasks/.../execute` | Job angelegt, Fortschritt 0 → 100 %, Aufgabe `done`, Audit-Eintrag |
| `POST /api/v1/tasks/.../execute` bei Hochrisiko ohne Freigabe | **409 Conflict** mit klarer Meldung |
| `POST /api/v1/tasks/.../approve` | Aufgabe freigegeben (`approved=true`) |
| Freigabe + Ausführung Core-Update | Job `succeeded`, Ergebnis „6.5.5 → 6.6.2“, Health-Check |
| `POST /api/v1/timer/start` | Timer aktiv, Kunde/Aufgabe korrekt |
| `POST /api/v1/timer/stop` | Dauer berechnet, Zeiteintrag geschlossen |
| `POST /api/v1/time-entries` | manueller Eintrag inkl. Dauer und Abrechenbarkeit |
| `GET /api/v1/billing/summary` | Pauschalen, Inklusivstunden, Überhang, Summe |
| `GET /api/v1/export/time-entries.csv` | 10 Zeilen, UTF-8-BOM, Semikolon, deutsches Format |
| `GET /api/v1/integrations` | Mock-Modus, Basis-URL, letzter Sync, Webhook-Hinweis |

### Browser-Abnahme

Alle sieben Hauptansichten wurden im Browser geladen und visuell geprüft:

- Dashboard
- Kundenliste
- Kundendetail
- Wartungslauf-Liste
- Wartungslauf-Detail
- Zeiten und Abrechnung
- Integrationen

Zusätzlich wurde die Aufgabenausführung im Browser beobachtet: Klick auf „Ausführen“, Job-Fortschritt mit Zwischenmeldung, Abschluss, Ergebnisanzeige und Aktualisierung des Ausführungsprotokolls. Der Timer wurde über die Oberfläche gestartet und gestoppt; die globale Timer-Leiste, der laufende Sekundenzähler, der Stop-Button und der erzeugte Zeiteintrag waren sichtbar.


### WordPress-Agent

- REST-Routen für Health, Inventar, Plugin-Updates, Core-Updates und Log vorhanden.
- HMAC über Zeitstempel und Raw Body, Application Password, Capability-Prüfung, IP-Allowlist, Rate-Limit.
- Backup-Sperre: Updates ohne `backup_confirmed=true` werden mit 409 abgelehnt.
- Dry-Run, Health-Check, Fatal-Error-Logging, Job-Marker.
- WP-CLI-Fallback dokumentiert.
- Signierter Testclient (`test-request.mjs`) vorhanden.

## 4. Nicht verifiziert

| Punkt | Grund | Konsequenz |
| --- | --- | --- |
| Live-NinjaOne | Kein Tenant/API-Zugang in dieser Umgebung | Live-Test-Checkliste in [docs/ninjaone-api.md](ninjaone-api.md) abarbeiten |
| `script/run` mit Machine-Token | Nur mit echtem Tenant prüfbar | Wichtigster offener Punkt; ggf. Benutzerkontext-Token |
| WordPress-Agent zur Laufzeit | Kein PHP/WordPress in der Umgebung | Auf Staging-Site mit Backup testen |
| Dockerfile/Compose | Docker nicht installiert | Build und Healthcheck auf Zielumgebung testen |
| Tauri | Bewusst nicht gebaut | Option für Phase 3 |
| DATEV-Import | Format mit Steuerberater abzustimmen | CSV ist neutraler Leistungsnachweis |
| Mehrbenutzerbetrieb | Login/RBAC ist Phase 1 | Demo nutzt festen Benutzer |

## 5. Technische Erkenntnisse

1. **Die NinjaOne-API ist ausreichend für den geplanten Workflow.** Organisationen, Geräte, Patch-Scan/Apply, Script-Run und Jobs decken die Wartung ab. Die API bleibt asynchron; das Cockpit muss Job-Status und Timeouts sauber abbilden.
2. **Der eine Webhook pro Tenant ist ein Architekturfaktor.** Wenn er belegt ist, braucht das Cockpit Polling. Das ist im Konzept berücksichtigt, aber noch nicht implementiert.
3. **WordPress-Core-REST reicht nicht für Updates.** Der eigene Agent ist keine Bequemlichkeit, sondern Voraussetzung. Die Backup-Sperre ist bewusst hart.
4. **Asynchrone Jobs tun der Oberfläche gut.** Der Job-Fortschritt macht Patch- und Script-Läufe nachvollziehbar, statt einen Spinner über einen langen HTTP-Request zu legen.
5. **SQLite trägt den PoC, aber nicht den Mehrbenutzerbetrieb.** Parallele Timer und Job-Updates brauchen PostgreSQL mit sauberem Locking.
6. **Die Trennung von Maschinenzeit und Menschzeit ist wichtig.** Sie verhindert, dass automatische Job-Laufzeiten unbemerkt abrechenbar werden.
7. **Die Abrechnung ist bewusst einfach.** Pauschale plus Überhang ist erklärbar; Übertrag und Staffeln sind Phase 3.

## 6. Bewusste Lücken

- Kein Login, keine Rollen, keine Session-Härtung.
- Keine verschlüsselte Credential-Tabelle; NinjaOne-Credentials kommen aus Umgebungsvariablen.
- WordPress-Ausführung aus dem Backend ist noch nicht verdrahtet; der Mock simuliert sie.
- Kein NinjaOne-Webhook-Empfänger, kein Polling bis zum endgültigen Job-Ergebnis.
- Keine PDF-Reports, kein DATEV-Feinformat, keine sevDesk-/Lexware-Adapter.
- Keine Mandantenfähigkeit, kein Kundenportal, kein Offline-Timer.
- CORS ist im PoC permissiv; für Produktion einschränken.

## 7. Empfehlung für Phase 1

**Reihenfolge:**

1. **NinjaOne-Live-Test** mit echtem Tenant und Testgerät. Insbesondere `script/run` mit Machine-Token, Patch-Jobs und Job-Polling. Ergebnis dokumentieren.
2. **WordPress-Agent auf Staging** installieren, Backup ziehen, Dry-Run, ein unkritisches Plugin aktualisieren, Health-Check prüfen.
3. **Monatsbericht statt PSA-Adapter:** Die Rechnungsstellung läuft manuell. Der Bericht ist der Kern, ein DATEV-/sevDesk-/Lexware-Adapter entfällt.
4. **Phase 1 bauen:** Login/RBAC, PostgreSQL, verschlüsselte Credentials, echte Agent-Verdrahtung, Sync-Konflikte, PDF-Basisbericht.
5. **Erst danach** Automation (Webhooks, geplante Läufe) und PSA-Adapter.

**Nicht bauen, bevor die Live-Tests grün sind:** PDF-Design, Kundenportal, Tauri, Mobile-App.

## 8. Demo zurücksetzen

```bash
rm -f data/cockpit.db data/cockpit.db-shm data/cockpit.db-wal
npm run dev:api
```

Beim Start werden die Demo-Daten neu angelegt.

## 9. Artefakte

- [README](../README.md) – Schnellstart und Überblick
- [Konzept](konzept.md) – Produkt, Workflow, Abrechnung, Roadmap
- [NinjaOne-API](ninjaone-api.md) – Endpunkte und Live-Test-Checkliste
- [WordPress-Agent](../packages/wp-agent/README.md) – Installation und Sicherheit
- [ADRs](adr/) – Architekturentscheidungen
