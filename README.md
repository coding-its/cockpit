# Wartungscockpit

Ein Phase-0-Prototyp für kleine Systemhäuser: Wartungsläufe für **NinjaOne** und **WordPress** planen, ausführen, dokumentieren und die aufgewendete Zeit abrechnen.

> **Kernidee:** NinjaOne bleibt Inventar- und Ausführungsquelle. WordPress bekommt einen kleinen Maintenance-Agenten. Das Cockpit ist die Schicht dazwischen – für Checklisten, Freigaben, Timer, Nachweise und Abrechnung. Kein zweites RMM, kein Helpdesk, kein Monitoring-Produkt.

## Desktop-Anwendung (Tauri)

Das Cockpit läuft als Windows-Anwendung: ein Fenster, kein Browser, **kein Server**.
Die Axum-Routen laufen im selben Prozess wie das Fenster (`apps/desktop/src-tauri`); die
Oberfläche ruft sie über einen Funktionsaufruf (`invoke("api_call", …)`) statt über HTTP. Es
gibt keinen Port, keinen Dienst und nichts, womit sich ein anderer Rechner verbinden könnte –
die Datenbank ist die einzige Verbindung nach außen.

```bash
npm run dev:desktop        # bauen und starten
npm run installer:desktop  # MSI erzeugen
```

Ergebnis: `apps/desktop/src-tauri/target/release/wartungscockpit.exe` (14 MB) und
`…/bundle/msi/Wartungscockpit_0.1.8_x64_de-DE.msi` (5,5 MB).

**Nach der Installation** sucht die App ihre Einstellungen in dieser Reihenfolge:
`%DB_CONFIG%`, `data\db.json` im Arbeitsverzeichnis, `data\db.json` in den Elternordnern der
Programmdatei, dann `%APPDATA%\Wartungscockpit\db.json`. Dorthin gehören nach einer MSI-Installation
`db.json` und `.env` – unter `Program Files` darf niemand schreiben.

**Der Browserbetrieb bleibt erhalten** (`npm run dev:api`, Port 8080) – nützlich für schnelle
Entwicklung und für Rechner ohne Installation. Beide Wege benutzen dieselben Routen, dieselbe
Rechteprüfung und denselben Audit-Trail; nur der Transport unterscheidet sich.

## Mehrplatzbetrieb

Die Anwendung ist für mehrere Personen gleichzeitig gebaut:

- **Eigener Timer je Person** – `time_entries.user_id` ist der Bezug, nicht der Name.
- **„Gerade in Arbeit"** unter *Zeiten* zeigt, wer gerade woran sitzt.
- **Rollen**: Administrator, Techniker, Nur-Lesen (Praktikanten).
- **Jede Person installiert die App** und verbindet sie mit derselben PostgreSQL-Datenbank.

Zwei Dinge brauchen Administratorrechte und sind bewusst nicht automatisch eingerichtet: der
Eintrag fürs Firmennetz in `pg_hba.conf` und eine eingehende Firewall-Regel für TCP 5432 auf dem
Datenbankrechner.

## Tests

Die Datenbanktests laufen gegen eine eigene Datenbank, damit keine echten Zeiten angerührt werden:

```bash
cd apps/api
TEST_DATABASE_URL="postgres://benutzer:passwort@127.0.0.1:5432/wartungscockpit_test" cargo test
```

Die Datenbank muss vorher angelegt sein; die Tabellen legt der Test selbst an und leert sie zu Beginn jedes Laufs.

## Was der Prototyp kann

- Kunden und Geräte aus NinjaOne importieren – **live** (39 Kunden, 306 Geräte im Test-Tenant) oder im Mock-Modus
- **Fokus-Kunden**: große Kunden in der Übersicht, kleine bleiben verwaltet und per Augen-Symbol ausblendbar
- **Websites als eigener Zweig**: eigener Kundenstamm für WordPress, getrennt von NinjaOne – mit Kontaktdaten, Stundensatz, Login-Daten, Aktionen und Zeiten
- **Remote-Sprung**: pro Gerät ein Klick in die NinjaOne-Geräteseite, um dort die Remote-Session zu starten
- **TeamViewer direkt**: TeamViewer-ID je Gerät pflegbar, ein Klick startet die Session über `teamviewer10://control?device=<ID>` – ohne Umweg über NinjaOne
- **Kommentare zu Zeiten**: pro Eintrag erfassbar und direkt in der Liste editierbar
- **Anmeldung mit Rollen**: Administrator, Techniker, Nur-Lesen – Argon2id-Passwörter, HttpOnly-Session-Cookies, Anmeldung per Benutzername oder Anzeigename
- Geräte mit Typ, OS-Klasse, Online-Status und letztem Kontakt
- **Patchstand und Antivirus live aus NinjaOne** (`/v2/queries/os-patches`, `/v2/queries/software-patches`, `/v2/queries/antivirus-status`): offene und abgelehnte Patches aus Betriebssystem und Software getrennt, aktives Antivirus-Produkt und Signaturenstand; „unbekannt" nur, wo NinjaOne nichts liefert
- **Websites bearbeiten und löschen** im Web-Kunden – Passwörter bleiben erhalten, wenn man sie leer lässt, und Zeiten überleben das Löschen
- **Web-Kunden löschen oder deaktivieren**; bei vorhandenen Zeiten verweigert das Cockpit das Löschen, damit die Abrechnung nicht still verändert wird
- **Einstellungen** statt reiner Benutzerverwaltung: Benutzer und Stundensätze/Umsatzsteuer pflegbar
- **Automatischer Timer-Start** beim Öffnen eines Geräts oder einer Website-Vorschau – ein laufender Timer wird dabei nie unterbrochen
- **Website-Vorschau** im Web-Kunden mit iFrame; Klick öffnet die Seite im neuen Tab und startet die Zeit
- **Kommentar-Erinnerung an zwei Stellen**: beim Stoppen eines Timers (Kopfleiste, Geräte-Panel, Panel-Schließen) und beim Speichern einer geänderten Website – der Dialog hängt den Text an den Eintrag bzw. legt einen mit Dauer an
- **Build-Stand** in der Seitenleiste, damit nach einem Update sofort sichtbar ist, welche Version läuft
- Schnelltest pro Gerät: eingebaute NinjaOne-Aktion (nur nach Freigabe) und Patch-Scan direkt aus der Kundenansicht
- **Aktionen am Gerät aus der Kundenansicht**: OS-Scan, Software-Scan, OS-Updates, Software-Updates, Neustart (mit Begründung, optional erzwungen) und Wartungsmodus – jeweils mit Rückfrage, Sperre gegen Parallel-Läufe und Abschluss aus der NinjaOne-Gerätehistorie; Fehlschläge stehen als Fehler, nicht als „OK"
- Zwei OAuth-Wege: Machine-Token für Inventar und Patches, Benutzerkontext für Script-Aktionen
- Nachweise aus der echten NinjaOne-Gerätehistorie (`/activities`) inklusive Script-Ausgabe, nicht nur „erfolgreich" aus dem HTTP-Status
- WordPress-Sites mit Versionen, Plugin-Updates, Core-Status und Backup-Alter verwalten
- Wartungsläufe mit Checkliste, Risiko-Freigabe und Ausführungsprotokoll
- Einsätze für spontane Arbeiten: Priorität, Kanal, Fälligkeit, gleiche Zeiterfassung
- NinjaOne-Patch-/Script-Aufgaben asynchron ausführen und den Job-Fortschritt verfolgen
- WordPress-Agent **live**: Health-Check, Plugin-Updates mit Dry-Run, Backup-Sperre und Health-Check danach
- Timer pro Aufgabe, manuelle Zeiteinträge, Maschinenzeit – **pro Person getrennt**, mehrere dürfen gleichzeitig messen; „Gerade in Arbeit" zeigt, wer woran sitzt
- Vertragslogik mit Inklusivstunden, Überhang und Pauschale
- CSV-Export für die Abrechnung, Audit-Trail, Health-Status
- **PostgreSQL statt SQLite**: mehrere Personen gleichzeitig, Verbindungsdaten in der Oberfläche änderbar (Einstellungen → Datenbank), mit Verbindungstest
- **Monatsbericht** als druckbares Dokument (PDF über den Browser): Zusammenfassung je Kunde mit Überhang, Satz, Pauschale, netto und brutto, darunter je Kunde die Wartungsläufe, Website-Aktionen und Zeiten mit Kommentar – gesamt oder **pro Kunde druckbar**
- **Aufstellung offener Zeiten je Kunde**: alle noch nicht abgerechneten Zeiten – auch über einen Monatswechsel – als druckbares Dokument mit Summe, USt und Brutto, danach **als abgerechnet markieren**; markierte Einträge erscheinen in der Zeitenliste als „abgerechnet"
- **Abrechnungskonten**: mehrere Datensätze (NinjaOne, Web, später TeamViewer) unter einem Kürzel zu einer Rechnung zusammenfassen – ein Vertrag, ein Satz, eine Summe; ohne Konto bleibt alles wie bisher
- **Abrechnung sekundengenau**: Betrag je Position = Dauer × Stundensatz, auf Cent gerundet, Summe = Addition der Positionen – keine Mindestzeiten, keine Aufrundung; Zeitenliste, Aufstellung, Monatsbericht und CSV rechnen mit derselben Funktion
- **Standard-Stundensatz** 126,05 € netto (150 € brutto), wenn im Vertrag keiner gepflegt ist; im Bericht als „Standard" gekennzeichnet
- **tix-Verlinkung** aus Einsätzen und Integrationen (das System hat keine API)
- Responsive Vue-3-Oberfläche, installierbar als PWA (Manifest vorhanden)

## Schnellstart

Voraussetzungen: Node 20+, Rust 1.85+, npm und **PostgreSQL 16 oder neuer**. Ohne erreichbare
Datenbank startet die API nicht – die Verbindung steht in `data/db.json` oder unter
Einstellungen → Datenbank.

```bash
# 1. Datenbank anlegen (einmalig, als PostgreSQL-Administrator)
psql -U postgres -c "CREATE ROLE cockpit LOGIN PASSWORD 'eigenes-passwort'"
psql -U postgres -c "CREATE DATABASE wartungscockpit OWNER cockpit ENCODING 'UTF8'"
psql -U postgres -c "CREATE DATABASE wartungscockpit_test OWNER cockpit ENCODING 'UTF8'"

# 2. Projekt
cd wartungscockpit
npm install
cp .env.example .env

# 3. Verbindung hinterlegen (Passwort aus Schritt 1)
cat > data/db.json <<'JSON'
{ "host": "127.0.0.1", "port": 5432, "database": "wartungscockpit",
  "user": "cockpit", "password": "eigenes-passwort" }
JSON

# 4. Starten – die Tabellen legt die API selbst an
cd apps/api && cargo run
```

Altdaten aus der SQLite-Zeit übernimmt `scripts/import-sqlite-to-postgres.py`
(erzeugt eine SQL-Datei, die mit `psql -f` eingespielt wird).

Für echte NinjaOne-Daten in `.env` eintragen:

```env
NINJAONE_MOCK=false
NINJAONE_BASE_URL=https://eu.ninjarmm.com
NINJAONE_CLIENT_ID=…
NINJAONE_CLIENT_SECRET=…
NINJAONE_SCOPE="monitoring management"

# Nur für Script-Aktionen nötig (App-Typ Webanwendung):
NINJAONE_USER_CLIENT_ID=…
NINJAONE_USER_CLIENT_SECRET=…
```

Ohne diese Werte startet die App automatisch im Mock-Modus mit Demo-Daten.

**Wichtig:** Werte mit Leerzeichen müssen in Anführungszeichen stehen. Ohne Quotes bricht der Parser ab und ignoriert alle folgenden Zeilen der `.env`.

Für die Arbeit am Code zwei Terminals (schnelles Nachladen, kein Neubau):

```bash
# Terminal 1 – API auf Port 8080 (Browserbetrieb)
npm run dev:api

# Terminal 2 – Vite auf Port 5173
npm run dev:web
```

Dann [http://127.0.0.1:5173](http://127.0.0.1:5173) öffnen.

Die fertige Anwendung starten bzw. installieren:

```bash
npm run dev:desktop        # Fenster-Anwendung direkt starten
npm run installer:desktop  # MSI nach …/bundle/msi/ bauen
```

Die Datenbank ist eine PostgreSQL-Datenbank; die Verbindung steht in `data/db.json`. Zum Zurücksetzen die Tabellen leeren und die API neu starten – dann werden die Demo-Daten erneut angelegt. Altdaten aus der SQLite-Zeit übernimmt `scripts/import-sqlite-to-postgres.py`.

## Demo in drei Minuten

1. **Übersicht** – KPIs, anstehende Wartungen, Handlungsbedarf, Zeiten pro Kunde.
2. **Wartungen → Nordlicht Dental** – Checkliste öffnen, „Neustartstatus prüfen“ ausführen. Der Job läuft mit Fortschritt, danach stehen Ergebnis, Dauer und Audit-Eintrag.
3. **Weber & Söhne → Wartungslauf** – „WordPress-Core auf 6.6.2 aktualisieren“ ist als Risiko hoch markiert. Erst „Freigeben“, dann ausführen.
4. **Timer** – bei einer Aufgabe „Timer“ starten; oben rechts läuft die Uhr. „Stop“ erzeugt einen Zeiteintrag.
5. **Zeiten** – Monatsübersicht, Vertragslogik, manueller Eintrag, CSV-Export.
6. **Integrationen** – NinjaOne-Sync testen, Mock-/Live-Modus, WordPress-Agent-Endpunkte, Audit-Trail.

## Architektur

```
┌──────────────────────────────┐
│ Vue 3 + TypeScript (PWA)     │
│ Vite · Pinia · Vue Router    │
└──────────────┬───────────────┘
               │ REST /api/v1
┌──────────────▼───────────────┐       ┌─────────────────────┐
│ Rust · Axum · SQLx           │──────▶│ NinjaOne Public API │
│ Worker für Ausführungen      │       │ OAuth 2.0 · v2      │
│ Audit · Timer · Abrechnung   │       └─────────────────────┘
└──────────────┬───────────────┘
               │ PostgreSQL
┌──────────────▼───────────────┐       ┌─────────────────────┐
│ Kunden, Sites, Runs, Tasks,  │──────▶│ WordPress-Agent     │
│ Zeiten, Jobs, Audit          │       │ Mu-Plugin · REST    │
└──────────────────────────────┘       └─────────────────────┘
```

- **Backend:** Rust, Axum, SQLx, Tokio, Reqwest. Ein modularer Monolith mit klarer Trennung von Routen, Ausführungsschicht, NinjaOne-Client und Abrechnung.
- **Frontend:** Vue 3, TypeScript, Vite, Pinia, Vue Router, Lucide-Icons. Eigene CSS-Tokens statt UI-Framework, damit die Oberfläche leicht anpassbar bleibt.
- **Datenbank:** PostgreSQL. SQLite war der Prototyp, taugt aber nicht für mehrere Personen gleichzeitig – eine Datei über eine Netzwerkfreigabe lässt sich nicht sicher beschreiben.
- **Jobs:** DB-gestützte Job-Tabelle, asynchrone Ausführung über `tokio::spawn`, Fortschritt per Polling. Kein Redis nötig.
- **WordPress:** Mu-Plugin mit Application Password, HMAC, IP-Allowlist, Dry-Run, Backup-Sperre und Health-Check. WP-CLI ist als Fallback vorgesehen.

## Projektstruktur

```
apps/api/            Rust-API, Datenmodell, Seed, NinjaOne-Client, Worker
apps/web/            Vue-3-SPA, Views, Komponenten, Styles
packages/wp-agent/   WordPress-Mu-Plugin + signierter Testclient
docs/                Konzept, API-Referenz, ADRs, Phase-0-Bericht
infra/               Dockerfile, Compose, Caddyfile
```

## API (Auszug)

| Methode | Pfad | Zweck |
| --- | --- | --- |
| GET | `/api/v1/dashboard` | KPIs, nächste Runs, Handlungsbedarf, Aktivität |
| GET | `/api/v1/customers` | Kundenliste mit Aggregaten |
| GET | `/api/v1/customers/{id}` | Kunde, Geräte, Sites, Verträge, Zeiten |
| GET | `/api/v1/runs` | Wartungsläufe, filterbar nach Status/Kunde |
| GET | `/api/v1/runs/{id}` | Lauf inklusive Checkliste und Protokoll |
| POST | `/api/v1/tasks/{id}/approve` | Risikoreiche Aufgabe freigeben |
| POST | `/api/v1/tasks/{id}/execute` | Aufgabe asynchron ausführen |
| GET | `/api/v1/jobs/{id}` | Job-Fortschritt |
| GET/POST | `/api/v1/timer/active`, `/timer/start`, `/timer/stop` | Zeiterfassung, jeweils für die angemeldete Person |
| GET | `/api/v1/timer/running` | alle laufenden Timer im Team |
| GET/POST | `/api/v1/time-entries` | Zeiteinträge lesen/anlegen |
| GET | `/api/v1/billing/summary?month=YYYY-MM` | Vertrags- und Abrechnungssicht |
| GET | `/api/v1/reports/monthly?month=YYYY-MM&include_idle=bool` | Monatsbericht als Dokument (Druck/PDF) |
| PATCH | `/api/v1/devices/{id}/teamviewer` | TeamViewer-ID eines Geräts setzen oder löschen |
| GET | `/api/v1/devices/{id}/patches` | gemeldete Patches eines Geräts, offene zuerst |
| POST | `/api/v1/devices/{id}/quick-action` | Aktion am Gerät: `patch-scan`, `software-scan`, `patch-apply`, `software-apply`, `reboot`, `maintenance-start`, `maintenance-stop`, `script` |
| PATCH/DELETE | `/api/v1/web-sites/{id}` | Website ändern bzw. entfernen (Zeiten bleiben) |
| PATCH/DELETE | `/api/v1/web-clients/{id}` | Web-Kunde deaktivieren bzw. löschen (Löschen nur ohne Zeiten) |
| GET/PUT | `/api/v1/settings/billing` | Stundensatz und Umsatzsteuer lesen/ändern |
| POST | `/api/v1/sync/monitoring` | Patch- und Antivirusstand neu holen (ohne Inventarabgleich) |
| GET | `/api/v1/export/time-entries.csv?month=YYYY-MM` | CSV-Export |
| POST | `/api/v1/sync/ninjaone` | Organisationen und Geräte abgleichen |
| GET | `/api/v1/integrations` | Integrationsstatus |

## Passwort vergessen

```cmd
apps\api\target\debug\wartungscockpit-api.exe --reset-admin-password
```

Gibt ein neues Passwort aus und beendet alle Sitzungen des Kontos.

## Tests und Build

```bash
cargo test --manifest-path apps/api/Cargo.toml   # 4 Tests
npm run build:web                                # vue-tsc + vite build
```

Der WordPress-Agent wurde mit einem PHP-Parser syntaktisch geprüft. Ein echter Lauf gegen eine WordPress-Instanz ist in dieser Umgebung nicht möglich, weil PHP fehlt. Die Anleitung und der signierte Testclient liegen bei.

## Dokumentation

- [Konzept](docs/konzept.md) – Produkt, Datenmodell, Workflow, Abrechnung, Roadmap
- [Live-Test-Protokoll](docs/live-test.md) – echter Tenant, `403 user_context_required`, nächste Schritte
- [XAMPP & WordPress lokal](docs/xampp-wordpress.md) – Einrichtung, Ergebnis, sechs Fallstricke
- [Bedienung](docs/bedienung.md) – Kunden ausblenden, Sites anlegen, Remote, Zeiterfassung
- [NinjaOne-API](docs/ninjaone-api.md) – Endpunkte, Fallstricke, Live-Test-Checkliste
- [WordPress-Agent](packages/wp-agent/README.md) – Installation, Endpunkte, Signatur, Sicherheit
- [ADRs](docs/adr/) – Architekturentscheidungen
- [Phase-0-Ergebnis](docs/phase-0-ergebnis.md) – was verifiziert ist und was nicht

## Status und Grenzen

Der Prototyp ist eine **Entscheidungsgrundlage**, kein produktionsreifes PSA.

**Live verifiziert:**

- NinjaOne: 39 Kunden, 306 Geräte, Patch-Scan (HTTP 204 + Aktivitätsnachweis), Script-Ausführung mit Benutzerkontext (DNS-Cache auf einem Testgerät, Ausgabe zurückgelesen)
- NinjaOne Monitoring: Patch- und Antivirusstand für 305 Geräte in ~3,5 s (126 mit Patchdaten, 10 Antiviruswarnungen)
- WordPress: lokale XAMPP-Instanz (7.1.3, Port 81), Health-Check und Plugin-Update 5.8 → 6.2.3 über das Cockpit

Bewusst nicht enthalten:

- verschlüsselte Credential-Verwaltung (Agent-Passwörter und Secrets liegen im PoC im Klartext in der Datenbank)
- NinjaOne-Webhook-Empfänger
- Script-Auswahl aus der NinjaOne-Bibliothek in der Oberfläche
- tix-Anbindung (kein API-Zugang vorhanden)
- Mandantenfähigkeit, Kundenportal, Offline-PWA

Live-Betrieb setzt voraus: API-Zugang im NinjaOne-Tarif, geklärte Webhook-Situation (nur ein Webhook pro Tenant), **Benutzerkontext für `script/run`** (Machine-Token wird dort abgelehnt), Backup-Strategie für WordPress und HTTPS.

## Lizenz

MIT – der vollständige Text steht in [LICENSE](LICENSE).

Kurz gesagt: Nutzen, verändern, weitergeben und verkaufen ist erlaubt, solange der Copyright-Hinweis in Kopien erhalten bleibt. Es besteht keine Pflicht, den Quellcode offenzulegen, und es wird keine Gewähr übernommen. Wer die Anwendung unverändert an Kunden weitergibt, darf das also tun – muss die Lizenz aber mitliefern.

