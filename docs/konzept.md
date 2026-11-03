# Wartungscockpit – Konzept

Stand: Oktober 2026 · Phase 0 umgesetzt

## 1. Ausgangslage und Empfehlung

Ein kleines Systemhaus betreut Kunden mit Windows-Umgebungen (NinjaOne) und WordPress-Websites. Die Wartung ist heute vermutlich über mehrere Werkzeuge, Tabellen und Zuruf verteilt. Genau dort entsteht der Aufwand, der später schwer abrechenbar ist: Wer hat wann welches Update installiert, warum wurde ein Update verschoben, wie lange hat es gedauert, und was davon ist im Wartungspaket enthalten?

**Empfehlung:** Kein neues RMM und kein vollständiges PSA bauen. NinjaOne bleibt das ausführende System für Geräte, Patches, Scripts und Policies. WordPress bekommt einen schlanken Agenten für Inventar und Updates. Dazwischen entsteht ein **Wartungscockpit** mit vier Kernfunktionen:

1. Wartungsläufe planen und mit Checklisten ausführen,
2. risikoreiche Schritte freigeben und auditieren,
3. Zeit pro Kunde, Lauf und Aufgabe erfassen,
4. Leistungen und Pauschalen abrechenbar exportieren.

Das ist die Schicht, die weder NinjaOne noch WordPress von sich aus liefern – und sie ist klein genug, um sie selbst zu betreiben.

## 2. Was die Schnittstellen wirklich können

### NinjaOne Public API v2

NinjaOne stellt eine öffentliche REST-API v2 mit OAuth 2.0 bereit. Die für das Cockpit relevanten Fähigkeiten:

- **Authentifizierung:** `POST /oauth/token` mit `grant_type=client_credentials`, Scopes `monitoring` und `management`. Einrichtung unter Administration → Apps → API → Client App IDs. Für EU-Mandanten ist `https://eu.ninjarmm.com` die Basis-URL.
- **Kunden und Geräte:** `GET /v2/organizations`, `GET /v2/organization/{id}/devices`, `GET /v2/device/{id}`. Damit lässt sich die Kunden- und Geräteliste regelmäßig abgleichen.
- **Patches:** `POST /v2/device/{id}/patch/os/scan|apply` und `POST /v2/device/{id}/patch/software/scan|apply`. Die Aufrufe sind asynchron und liefern typischerweise 204. Eine kuratierte Auswahl einzelner KBs bietet die API nicht; gesteuert wird „scannen“ und „offene Patches installieren“. Die Patch-Policy bleibt in NinjaOne.
- **Scripts:** `POST /v2/device/{id}/script/run` mit `scriptContent` oder `scriptId`, `runAs` SYSTEM/CURRENT_USER.
- **Jobs:** `GET /v2/device/{id}/jobs` für den Status asynchroner Ausführungen.
- **Tickets:** Log-Einträge inklusive `TIME_ENTRY` sind lesbar; beim Anlegen von Notizen gibt es ein `timeTracked`-Feld. Für Phase 1 ist interessant, ob Zeiten zusätzlich in NinjaOne gespiegelt werden sollen.
- **Webhooks:** `PUT /v2/webhook` mit `activities`-Filter, eigenen Headern und optionalem Secret. **Wichtig:** Es gibt genau eine Webhook-Konfiguration pro Tenant. Nutzt ein anderes Tool sie bereits, muss das Cockpit pollen oder den Empfänger ablösen.

**Bremsen und offene Punkte:**

- Rate-Limits sind nicht öffentlich beziffert. 429 muss mit Backoff behandelt werden; das ist im Client implementiert.
- In Community-Berichten wird dokumentiert, dass `script/run` mit einem reinen Machine-Token abgelehnt werden kann, weil NinjaOne einen echten Benutzer für den Audit-Trail erwartet. Das ist die wichtigste offene Frage für den Live-PoC.
- Die API liefert asynchrone Jobs, keine sofortigen Ergebnisse. Die Oberfläche muss mit „eingereicht/läuft“ umgehen können.
- Patch- und Script-Aktionen werden über NinjaOne-Policies und Berechtigungen gerahmt. Das Cockpit darf diese Policies nicht aushebeln.

### WordPress

Die Core-REST-API kann Plugins auflisten, aktivieren, deaktivieren, installieren und löschen. Sie kann **kein Plugin-Update auf eine neue Version** einspielen. Ebenso wenig gibt es einen Core-Update-Endpunkt, der für unbeaufsichtigte Wartung ausreicht.

Deshalb zwei Wege:

1. **Eigener Mu-Plugin-Agent** (empfohlen): registriert signierte REST-Routen, nutzt `wp_update_plugins()`, `Plugin_Upgrader` und `Core_Upgrader`, prüft `update_plugins` bzw. `update_core` und liefert Inventar und Health-Status zurück. Authentifizierung über Application Password (HTTPS) plus HMAC plus optionaler IP-Allowlist.
2. **WP-CLI über SSH** als Fallback: feste Befehle wie `wp plugin list --format=json`, `wp plugin update --all`, `wp core update`. Kein frei eingegebener Shell-Code. Sinnvoll für Hosts, die REST blockieren oder keinen Agenten erlauben.

Der Agent ist kein Backup-Ersatz. Updates werden nur mit `backup_confirmed=true` ausgeführt; das Cockpit muss vorher ein verifiziertes Backup nachweisen. Automatisches Rollback ist ohne Snapshot nicht möglich und wird bewusst nicht versprochen.

## 3. Produktabgrenzung

**Im Scope:**

- Kunden- und Geräte-Sync aus NinjaOne
- WordPress-Sites inklusive Versionen, Updates, Backup-Alter und Health
- Wartungsläufe mit Checklisten, Freigaben, Ausführungsprotokoll
- Zeiterfassung pro Aufgabe, manuelle Nachträge, Maschinenzeit
- Vertragslogik und Abrechnungsexport
- Audit-Trail und Wartungsnachweis

**Zwei getrennte Kundenstämme:**

- **NinjaOne-Kunden** (`customers`): Geräte, Patches, Scripts, Wartungsläufe.
- **Web-Kunden** (`web_clients`): WordPress-Websites, eigene Kontaktdaten, Stundensatz, Login-Daten, Aktionen und Zeiten.

Sie sind bewusst nicht verknüpft: nicht jeder WordPress-Kunde hat Geräte in NinjaOne und umgekehrt. Die Abrechnung führt beide in einer Übersicht zusammen.

**Seit dem Live-Test ergänzt:**

- **Einsätze** für spontane Arbeiten: Störung, Anfrage, Vor-Ort-Termin. Priorität, Eingangskanal, Fälligkeit, gleiche Zeiterfassung und dasselbe Protokoll wie Wartungsläufe.

**Nicht im Scope (Version 1):**

- Vollständiges Helpdesk (Mail-Eingang, SLA, Queues, Kundenportal, Ticketnummern-Kreis)
- Remote Control
- eigenes Monitoring oder Alarmierung
- Backup-Produkt
- Kundenportal
- vollständige Buchhaltung

Diese Abgrenzung ist wichtig. Ein halbes RMM ist teurer und schlechter als ein gutes Cockpit auf einem bestehenden RMM. Für Einsätze gilt dasselbe: ein halbes Helpdesk konkurriert mit NinjaOne-Ticketing und verliert.

## 4. Rollen und Nutzung

- **Techniker:** öffnet Wartungslauf, arbeitet Checkliste ab, startet Timer, dokumentiert Ergebnisse.
- **Teamleitung/Admin:** pflegt Kunden, Verträge, Wartungsfenster, Freigaben und Integrationen.
- **Buchhaltung:** prüft Zeiten, exportiert Leistungsnachweise, erzeugt Rechnungen.
- **Kunde (Phase 3):** erhält einen signierten Link zum Wartungsbericht, keine Login-Plattform.

Anmeldung mit Benutzername und Passwort (Argon2id), Sitzung als HttpOnly-Cookie. Drei Rollen: Administrator, Techniker, Nur-Lesen. Jeder Vorgang im Audit-Trail trägt den Namen des angemeldeten Benutzers.

## 5. Fachliches Datenmodell

| Entität | Zweck | Wichtige Felder |
| --- | --- | --- |
| `customers` | lokaler Kunde | `ninja_org_id`, Ansprechpartner, Notizen |
| `devices` | gecachtes NinjaOne-Gerät | `ninja_device_id`, OS, Patch-/AV-Status, online |
| `sites` | WordPress-Installation | URL, Agent-Modus, WP/PHP-Version, Updates, Backup-Alter |
| `contracts` | Wartungspaket | Intervall, Inklusivstunden, Stundensatz, Pauschale |
| `runs` | Wartungslauf pro Kunde/Periode | Status, Termin, Verantwortlicher, Notizen |
| `tasks` | Aufgabe im Lauf | Typ, Ziel, Risiko, Freigabe, Ergebnis |
| `task_executions` | jeder Ausführungsversuch | Request/Response, Dauer, Vorher-/Nachher-Version |
| `jobs` | asynchroner Job | Status, Fortschritt, Meldung |
| `time_entries` | Zeit | Kunde, Lauf, Aufgabe, Start/Ende, Dauer, abrechenbar, Quelle |
| `audit_events` | append-only Protokoll | Akteur, Aktion, Entität, Detail, Zeit |
| `app_settings` | Integrationszustand | letzter NinjaOne-Sync |

NinjaOne-IDs werden nur als Referenz gespeichert. Die App ist System of Record für Wartungsläufe, Zeiten und Nachweise; NinjaOne bleibt es für Geräte, Policies und Patch-Status.

## 6. Der Wartungslauf

Ein Lauf entsteht aus einem Vertrag und einem Intervall, zum Beispiel „zweiter Dienstag im Monat, 18:00–22:00 Uhr“. Die Checkliste kommt aus einer Vorlage:

- Patch-Scan auf dem Serversystem
- Sicherheitsupdates installieren
- Neustartstatus prüfen
- Antivirus-Status prüfen
- WordPress-Plugins aktualisieren
- WordPress-Core prüfen/aktualisieren
- Health-Check und Sichtprüfung
- Dokumentation

Jede Aufgabe hat ein Ziel, ein Risiko und eine Ausführungsart:

- `NINJA_PATCH` → NinjaOne Patch-Scan/Apply
- `NINJA_SCRIPT` → NinjaOne Script-Run
- `WP_PLUGIN_UPDATE` / `WP_CORE_UPDATE` → WordPress-Agent
- `WP_HEALTH` → Agent-Health-Check
- `MANUAL` → Bestätigung und Dokumentation

**Freigabe:** Aufgaben mit hohem Risiko (Major-Updates, Formular-/Shop-Plugins, Core-Sprünge) sind gesperrt, bis ein Techniker sie freigibt. Die Freigabe wird auditiert.

**Ausführung:** Die API legt einen Job an und antwortet sofort. Der Worker führt aus, schreibt Fortschritt und Ergebnis. Die Oberfläche pollt den Job und aktualisiert die Aufgabe. NinjaOne-Jobs laufen asynchron weiter; Phase 1 ergänzt das Polling bis zur endgültigen NinjaOne-Rückmeldung.

**Nachweis:** Nach dem Lauf entsteht automatisch ein Protokoll: Aufgabe, Ziel, Vorher-/Nachher-Version, Ergebnis, Dauer, Akteur. Phase 2 erzeugt daraus ein PDF für den Kunden.

## 7. Zeiterfassung und Abrechnung

### Erfassung

- **Timer pro Aufgabe:** Start/Stop, Pause, Wechsel zwischen Kunden. Ein laufender Timer pro Benutzer, sichtbar in der globalen Leiste.
- **Manuelle Einträge:** Datum, Dauer, Leistungsart, Beschreibung, abrechenbar ja/nein.
- **Maschinenzeit:** Laufzeit von NinjaOne-Jobs und WordPress-Updates wird getrennt erfasst. Sie ist nicht automatisch abrechenbar, sondern dient der Kalkulation und der Begründung von Pauschalen.
- **Keine stillen Zeiten:** Jede Ausführung erzeugt einen Audit-Eintrag; jede Zeit hat eine Quelle (`timer`, `manual`, `machine`).

### Vertragslogik

Ein Wartungspaket hat:

- monatliche Pauschale,
- Inklusivstunden pro Periode,
- Stundensatz für Überhang,
- Abrechnungstag.

Die Abrechnungssumme ist `Pauschale + max(0, abrechenbare Stunden − Inklusivstunden) × Stundensatz`. Kein Stundenübertrag in Version 1. Das ist bewusst einfach und für kleine Systemhäuser nachvollziehbar; Übertrag, Mindestlaufzeiten und Staffelpreise kommen später.

### Monatsbericht

Die Rechnungsstellung erfolgt manuell am Monatsende. Damit ist der **Monatsbericht pro Kunde** das eigentliche Abrechnungsprodukt – nicht ein PSA-Import. Er enthält:

- **Kopfdaten:** Kunde, Zeitraum, Vertrag, Pauschale, Inklusivstunden, Stundensatz
- **Leistungsübersicht:** jede Wartung mit Datum, Techniker, Aufgaben und Ergebnis
- **Änderungsnachweis:** Plugin- und Core-Versionen vorher/nachher, Patch-Stand
- **Zeitnachweis:** abrechenbare Stunden nach Leistungsart, nicht abrechenbare Zeit getrennt
- **Abrechnungssatz:** `Pauschale + max(0, abrechenbare Stunden − Inklusivstunden) × Stundensatz`
- **Besonderheiten:** verschobene Updates, blockierte Schritte, Ausfälle, offene Punkte
- **Geräte- und Site-Status** zum Monatsende
- **Unterschriftszeile** für den Kunden

Formate: **PDF** für die Kundenmappe, **XLSX/CSV** als Datengrundlage. Der CSV-Export bleibt als sekundäre Schnittstelle erhalten und hat weiterhin deutsches Format (Semikolon, Dezimalkomma, UTF-8-BOM).

Ein DATEV-, sevDesk- oder Lexware-Adapter ist **nicht** Teil der Roadmap. Falls sich das ändert, hängt sich ein Adapter an denselben Bericht an, ohne die Erfassung zu verändern.

## 8. Technische Architektur

### Stack

- **Frontend:** Vue 3, TypeScript, Vite, Pinia, Vue Router, Lucide. PWA-Manifest vorhanden; Service Worker und Offline-Timer folgen.
- **Backend:** Rust, Axum, SQLx, Tokio, Reqwest. OpenAPI kann aus den Routen generiert werden; im PoC noch nicht aktiviert.
- **Datenbank:** SQLite für den PoC, PostgreSQL 16 ab Phase 1. Das Schema nutzt TEXT-IDs, ISO-Zeitstempel und keine SQLite-spezifischen Datentypen, damit die Migration überschaubar bleibt.
- **Jobs:** Tabelle `jobs`, Worker mit `tokio::spawn`, Fortschritt per Polling. Für Phase 1: `SELECT … FOR UPDATE SKIP LOCKED` mit PostgreSQL, Retries, Timeouts, Idempotenzschlüssel.
- **Reports:** Typst-Templates (Phase 2), serverseitig gerendert, kein Chromium.
- **Deployment:** Docker Compose, Caddy für TLS, PostgreSQL-Volume, nächtliches `pg_dump` offsite.

### Ausführungsschicht

Ein gemeinsames Interface für Ausführungen:

```rust
trait Executor {
    async fn execute(&self, task: &Task) -> Result<ExecutionResult>;
}
```

Implementierungen: `NinjaOneExecutor`, `WordPressAgentExecutor`, `WpCliExecutor`, `ManualExecutor`. Jede Aufgabe erzeugt einen Job, jede Ausführung einen `task_executions`-Datensatz. Fehler werden als Ergebnis gespeichert, nicht verschluckt.

### NinjaOne-Client

- OAuth-Token-Cache mit Erneuerung 60 Sekunden vor Ablauf
- Retry mit exponentiellem Backoff bei 429 und 5xx, `Retry-After` wird respektiert
- Konfigurierbare Basis-URL für EU/US/Ozeanien/Kanada
- Mock-Modus mit realistischen Fixtures, damit die App ohne Zugangsdaten läuft
- Getrennte Methoden für Organisationen, Geräte, Patch-Scan/Apply, Script-Run und Jobs

### WordPress-Agent

- Mu-Plugin, keine Aktivierung nötig
- Application Password + HMAC + Zeitstempel + IP-Allowlist + Rate-Limit
- Inventar mit Plugin-/Theme-/Core-Updates
- Plugin- und Core-Updates nur mit Backup-Bestätigung
- Dry-Run, Health-Check, Fatal-Error-Logging
- WP-CLI-Fallback mit festen Befehlen

## 9. Sicherheit und DSGVO

- **Keine WordPress-Adminpasswörter.** Nur Application Passwords oder SSH-Keys, pro Site widerrufbar.
- **Secrets verschlüsselt at rest.** Im PoC kommen NinjaOne-Credentials aus Umgebungsvariablen; Phase 1 ergänzt eine verschlüsselte Credential-Tabelle mit Master-Key aus dem Secret Store.
- **Keine Secrets im Frontend.** Das Frontend sieht nur Statusinformationen.
- **Audit-Trail append-only.** Jede Ausführung, Freigabe, Timer-Aktion und jeder Sync wird protokolliert.
- **Least Privilege.** WordPress-Agent prüft `update_plugins`/`update_core`; NinjaOne-Scopes nur `monitoring` und `management`, kein `control`.
- **Wartungsfenster.** Änderungen an Kundensystemen nur im vereinbarten Fenster; risikoreiche Updates mit Freigabe.
- **Datenminimierung.** Kunden-, Geräte- und Site-Daten nur, soweit für Wartung und Abrechnung nötig.
- **EU-Hosting, AVV, Löschkonzept.** Zeiten und Reports brauchen definierte Aufbewahrungsfristen.
- **Backups.** Nächtliche Datenbank-Sicherung, verschlüsselt, offsite; Wiederherstellungstest dokumentieren.

## 10. Roadmap und Aufwand

### Phase 0 – PoC (umgesetzt)

- Monorepo, Rust-API, Vue-3-Oberfläche
- SQLite-Schema und Demo-Daten
- NinjaOne-Client mit OAuth, Retry, Mock-Modus
- Ausführung mit Job-Fortschritt und Audit
- Timer, Zeiteinträge, Vertragslogik, CSV
- WordPress-Agent als Code, syntaktisch geprüft
- Konzept, API-Referenz, ADRs

### Phase 1 – MVP (4–6 Wochen)

- Anmeldung mit Rollen (umgesetzt)
- PostgreSQL und Migration
- verschlüsselte Credential-Verwaltung
- echte WordPress-Agent-Verdrahtung inklusive Site-Konfiguration
- NinjaOne-Live-Test: `script/run`, Patch-Jobs, Job-Polling bis Abschluss
- Kunden-/Geräte-Sync mit Konfliktauflösung
- Wartungslauf-Vorlagen und wiederkehrende Termine
- Monatsbericht pro Kunde als PDF, CSV als Datengrundlage

### Phase 2 – Automation (3–4 Wochen)

- NinjaOne-Webhook-Empfänger oder Polling-Fallback
- geplante Läufe und Erinnerungen
- Health-Check-Automation, Backup-Verifikation
- PDF-Berichte mit Vorher-/Nachher-Vergleich
- Freigabeworkflow mit Vier-Augen-Prinzip
- Vulnerability-Feed (WPScan/Patchstack) optional

### Phase 3 – Ausbau (2–3 Wochen)

- Vertragslogik mit Übertrag/Staffeln, falls die Verträge das brauchen
- Berichtsvorlagen pro Kunde und Sammelbericht über alle Kunden
- Kundenportal mit signierten Links
- PWA-Offline-Timer
- Tauri-Hülle optional

Ein PSA-Adapter (DATEV, sevDesk, Lexware Office) ist bewusst nicht eingeplant. Die Rechnungsstellung läuft manuell auf Basis des Monatsberichts.

**Realistische Schätzung:** erste nutzbare Version in 6–8 Wochen, produktionsreif in 10–12 Wochen für eine Person. Kein Wochenendprojekt, weil Credential-Handling, Update-Sicherheit und Abrechnung sauber sein müssen.

## 11. Risiken

| Risiko | Wirkung | Gegenmaßnahme |
| --- | --- | --- |
| NinjaOne-API nicht im Tarif enthalten | Live-Betrieb blockiert | Tarif und API-Freigabe vor Phase 1 klären |
| `script/run` mit Machine-Token abgelehnt | Script-Aufgaben fallen aus | PoC mit echtem Tenant; ggf. Benutzerkontext-Token |
| Ein Webhook pro Tenant belegt | Keine Echtzeit-Events | Polling-Fallback, bestehende Integration prüfen |
| WordPress-Update bricht Site | Kundenausfall | Backup-Pflicht, Dry-Run, Health-Check, Freigabe, Snapshot |
| Hoster blockiert REST/SSH | Site nicht managebar | Zwei Agent-Modi, klare Fehlermeldung, manuelle Aufgabe |
| Rate-Limits/async Jobs | UI wirkt hängend | Queue, Backoff, Job-Status, keine synchronen UI-Aufrufe |
| Scope Creep Richtung RMM | Projekt wird unfertig | Harte Abgrenzung, kein Helpdesk/Monitoring/Remote |
| Credential-Leak | schwerer Vorfall | Verschlüsselung, Rotation, Audit, kein Frontend-Zugriff |

## 12. Build vs. Buy

NinjaOne hat selbst Ticketing/PSA-Funktionen; WP Umbrella oder MainWP decken WordPress-Wartung gut ab. Ein Selbstbau lohnt sich nur, wenn der Kernnutzen in der **einheitlichen Zeiterfassung, den eigenen Checklisten und den eigenen Nachweisen** liegt. Genau dafür ist der PoC geschnitten: Er zeigt in wenigen Tagen, ob die Integrationen tragen und ob der Workflow zum Team passt. Wenn nicht, ist die Alternative „NinjaOne-PSA + WordPress-SaaS“ billiger.

## 13. Offene Entscheidungen

1. Ist der NinjaOne-Webhook im Tenant frei?
2. Funktioniert `script/run` mit dem vorhandenen API-App-Typ?
3. Welche WordPress-Hoster erlauben REST, welche brauchen WP-CLI?
4. Welches Backup-Produkt gilt als „verifiziert“?
5. Welche Form braucht der Monatsbericht für die manuelle Rechnungsstellung – PDF-Layout, XLSX oder beides?
6. Sollen Zeiten zusätzlich in NinjaOne-Tickets gespiegelt werden?
7. Braucht es ein Kundenportal oder nur PDF-Links?
8. On-Prem-Server oder EU-VPS, und wer betreibt die Backups?

## Quellen

- [NinjaOne Public API](https://www.ninjaone.com/docs/application-programming-interface-api/public-api-operations/)
- [NinjaOne OpenAPI-Spezifikation](https://app.ninjarmm.com/apidocs/NinjaRMM-API-v2.json)
- [NinjaOne Webhooks](https://app.ninjarmm.com/apidocs-beta/core-resources/articles/webhooks/overview)
- [WordPress Plugins REST API](https://developer.wordpress.org/rest-api/reference/plugins/)
- [MainWP REST API](https://docs.mainwp.com/api-reference/rest-api/updates)
- [sevDesk API](https://tech.sevdesk.com/)
- [Lexware Office API](https://developers.lexware.io/)
