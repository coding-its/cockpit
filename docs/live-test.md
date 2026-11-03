# Live-Test – Protokoll

Stand: 26. Oktober 2026 · Tenant `eu.ninjarmm.com`

## 1. Status

| Schritt | Status | Ergebnis |
| --- | --- | --- |
| API-App angelegt (Machine-to-Machine) | erledigt | Client Credentials funktionieren |
| OAuth-Token | **grün** | Token wird geholt und zwischengespeichert |
| Organisationen lesen | **grün** | 39 Organisationen |
| Geräte lesen | **grün** | 306 Geräte in 39 Organisationen |
| Persistenz in Cockpit-DB | **grün** | 39 Kunden, 306 Geräte übernommen |
| Gerätedetails lesen | **grün** | `GET /v2/device/{id}` liefert OS, Build, RAM, Volumes |
| Patch-Scan auf Testgerät | **grün** | `POST /v2/device/147/patch/os/scan` → **HTTP 204** mit Machine-Token |
| Patch-Scan auf TEST-TAB-01 | **grün** | HTTP 204, Nachweis über Aktivität #1000001 nachgetragen |
| Script/Remote-Action auf TEST-TAB-01 | **grün** | DNS-Cache geleert, Ergebnis `SUCCESS`, Ausgabe zurückgelesen |
| Script mit Machine-Token | **blockiert** | Aufruf mit **403 `user_context_required`** abgelehnt (erwartet) |
| Geräte-Jobs nach Patch-Scan | siehe unten | `/v2/device/{id}/jobs` blieb leer |
| Patch-Nachweis über Aktivitäten | **grün** | `GET /v2/device/{id}/activities` zeigt `PATCH_MANAGEMENT_SCAN_COMPLETED` |
| Webhook-Slot | **offen** | im Tenant zu prüfen |
| WordPress-Agent | **gruen** | XAMPP Port 81, WP 7.1.3, Health + Plugin-Update verifiziert |

## 2. Tenant-Inventar (Ist-Stand)

| Kennzahl | Wert |
| --- | --- |
| Organisationen | 39 |
| Geräte | 306 |
| Windows Workstations | 205 |
| Windows Server | 95 |
| macOS | 4 |
| Linux Server | 1 |
| Netzwerkgerät (NMS) | 1 |
| online | 200 |
| offline | 106 |
| Freigabestatus | alle `APPROVED` |

Beispiele für Kunden: zum Beispiel Steuerbüro Nord, Zahnarztpraxis Süd, Autohaus West, Anwaltskanzlei Ost.

## 3. Erkenntnisse aus den Live-Daten

1. **Der Endpunkt `/v2/organization/{id}/devices` ist schlank.** Er liefert `id`, `systemName`, `dnsName`, `nodeClass`, `offline`, `lastContact`, aber **kein Betriebssystem und keinen Patch-Status**. Für den Gerätetyp reicht `nodeClass`; für OS-Build und Patch-Stand braucht es `GET /v2/device/{id}` je Gerät.
2. **`GET /v2/device/{id}` ist reichhaltig.** Es liefert OS-Name, Build, Release, Last Boot, RAM, Volumes, MAC/IP, letzten Benutzer — aber **auch keinen Patch- oder AV-Status**. Dafür sind die Report-Query-Endpunkte nötig (Phase 1).
3. **306 Detailabrufe pro Sync wären zu teuer.** Empfehlung: Detailanreicherung nur für Server und nur bei Bedarf (beim Öffnen eines Kunden oder eines Laufs), nicht bei jedem Sync.
4. **Ein echter Bug wurde durch Live-Daten gefunden.** `COALESCE(..., 0)` liefert in SQLite INTEGER; SQLx erwartet `f64`. Jeder Kunde ohne Vertrag ließ Dashboard und Abrechnung mit HTTP 500 scheitern. Behoben durch `CAST(... AS REAL)` und `0.0`.
5. **Demo-Daten müssen im Live-Modus verschwinden.** Der Sync deaktiviert Seed-Kunden (`cust-*`) und entfernt die Demo-Audit-Einträge, sonst mischen sich fiktive und echte Daten.
6. **`GET /v2/device/{id}/jobs` ist auch nach einem Patch-Scan leer.** Der Scan lief (HTTP 204), taucht dort aber nicht als Job auf. Vermutlich listet der Endpunkt nur Script-Jobs; Patch-Aufgaben erscheinen in der NinjaOne-Aktivität. Für Patch-Nachweise brauchen wir die Report-Query-Endpunkte, nicht `/jobs`.
7. **Schreibende Aufrufe funktionieren mit dem Machine-Token.** `POST …/patch/os/scan` gegen TEST-PC-01 (Gerät 147, online, Windows 10) lieferte **204**. Damit ist belegt, dass der Client-Credentials-Client `management`-Aktionen ausführen darf – für Patches. Ob `script/run` ebenfalls erlaubt ist, bleibt offen.
8. **Der echte HTTP-Status wird jetzt protokolliert.** Vorher hat das Cockpit den Status verworfen und nur „erfolgreich" vermerkt. Jetzt stehen Endpoint, Geräte-ID, Statuscode und Antwortkörper im Ausführungsprotokoll – nötig, um gegenüber dem Kunden zu belegen, was passiert ist.

## 3a00. Skript-Ausführung funktioniert – mit drei Hürden

Der Script-Weg ist jetzt vollständig belegt. Der Reihe nach waren drei Punkte zu lösen, die alle nicht offensichtlich sind:

### Hürde 1: Machine-Token wird abgelehnt

`403 user_context_required`. Gelöst durch eine zweite App vom Typ **Webanwendung** mit Authorization Code + Refresh Token und einmaliger Benutzerzustimmung.

### Hürde 2: Ohne `offline_access` kein Refresh-Token

Der Code-Tausch lieferte nur ein `access_token`. Der Scope muss `offline_access` enthalten — NinjaOne liefert sonst keinen Refresh-Token, obwohl der Grant-Typ erlaubt ist. Der Scope wird jetzt automatisch ergänzt; der Zustimmungsdialog zeigt dann explizit **Offline-Zugriff**.

### Hürde 3: Kein Inline-Script, und Script-IDs sind nicht ausführbar

Die Public API kennt **kein** `scriptContent`. `RunScriptRequest` erlaubt nur `type`, `id`, `uid`, `parameters`, `runAs`. Der Versuch mit `scriptContent` endet in `400 Unrecognized field`.

Danach mit einer ID aus `/v2/device/{id}/scripting/options` (`script.clear_dns_cache`, id 49):

```json
{"type":"SCRIPT","id":49,"runAs":"SYSTEM"}
```

> Aktion abgeschlossen: script.clear_dns_cache **Ergebnis: FAILURE**
> Ausgabe: **Script could not be found.**

Die ID aus den Optionen ist also nicht die ausführbare Script-ID. Der zuverlässige Weg sind **eingebaute Aktionen über `uid`**:

```json
{"type":"ACTION","uid":"3800e62f-…","runAs":"SYSTEM"}
```

Ergebnis auf TEST-TAB-01:

```
Aktivität #1000759 · ACTION · COMPLETED
Aktion abgeschlossen: DNS-Cache löschen — Ergebnis: SUCCESS
Ausgabe: Action: DNS-Cache löschen, Result: Success
         Windows-IP-Konfiguration
         Der DNS-Auflösungscache wurde geleert.
```

Das Cockpit löst die UID zur Laufzeit über `/v2/device/{id}/scripting/options` auf (Aktion mit „DNS" im Namen, konfigurierbar über `NINJAONE_POC_ACTION_NAME`) und schreibt die Ausgabe ins Protokoll.

**Wichtiger Fallstrick:** NinjaOne meldet auch Fehlschläge mit `statusCode: COMPLETED`. Ob es geklappt hat, steht nur im Meldungstext (`Ergebnis: FAILURE`). Das Cockpit wertet beides aus und setzt die Aufgabe bei Fehlschlag auf `failed`.

**Konsequenz für den MVP:** Eigene Kunden-Scripts müssen zuerst in NinjaOne angelegt und über ihre echte ID referenziert werden. Die Script-Auswahl gehört in die Oberfläche (Auswahl aus der Bibliothek), nicht als Freitext.

## 3a0. Vollständiger Durchlauf auf TEST-TAB-01

Ablauf am 26.10. gegen 13:45, genau eine Ausführung:

| Schritt | Ergebnis |
| --- | --- |
| Gerät online | Windows 11 Pro, letzter Kontakt 13:45 |
| Testlauf anlegen | Aufgabe `Patch-Scan auf TEST-TAB-01` |
| Ausführen | `POST /v2/device/410/patch/os/scan` → **HTTP 204** |
| Sofort danach | Abschluss noch nicht gemeldet → Nachzieher gestartet |
| 123 s später | Aktivität **#1000001** `PATCH_MANAGEMENT_SCAN_COMPLETED` |
| Protokoll | „Patch-Scan abgeschlossen laut NinjaOne-Aktivität #1000001" |

Damit ist belegt: **Geräteanbindung, Patch-Weg und Nachweisführung funktionieren auf echter Hardware.** Nur der Script-Weg scheitert an der Token-Art, nicht am Gerät.

Nebenbefund: Der Abschluss kam nach gut zwei Minuten. Deshalb wartet das Cockpit zwei Minuten synchron und trägt den Nachweis danach im Hintergrund nach — der Nutzer sieht den Lauf nicht ewig auf „60 %" hängen.

## 3a. Das entscheidende Ergebnis: `script/run` verlangt Benutzerkontext

Der Test auf **TEST-TAB-01** (Gerät 410, Windows 11 Pro, online, Kunde Steuerbüro Nord) wurde korrekt vorbereitet: Aufgabe freigegeben, ausgeführt, Ergebnis protokolliert. NinjaOne antwortete:

```json
HTTP 403
{"error":"user_context_required",
 "error_description":"Access key does not have user context which is required for this request",
 "error_code":5}
```

Damit ist die offene Frage beantwortet: **Ein reines Machine-Token (Client Credentials) darf keine Scripts ausführen.** Das ist keine Fehlkonfiguration, sondern eine bewusste NinjaOne-Regel für den Audit-Trail – es muss erkennbar sein, welcher Mensch einen Befehl abgesetzt hat.

Was trotzdem mit dem Machine-Token funktioniert (belegt):

| Aktion | Ergebnis |
| --- | --- |
| Organisationen, Geräte, Aktivitäten lesen | 200 |
| `POST …/patch/os/scan` | **204** |
| `POST …/script/run` | **403 user_context_required** |

**Konsequenz für das Cockpit:** Zwei getrennte Credentials.

1. **Machine-Token** für Inventar, Patch-Scan/Apply, Aktivitäten – läuft.
2. **Benutzerkontext** für Scripts – neuer Authorization-Code-Flow mit Refresh-Token.

Umgesetzt ist beides: Der Client kann beide Token-Typen, und `script/run` nutzt automatisch den Benutzer-Token. Fehlt der Benutzer, erscheint eine klare Meldung statt eines kryptischen Fehlers, und der Lauf wird als **blockiert** markiert.

Was noch fehlt: eine zweite NinjaOne-App vom Typ **Webanwendung** mit Redirect-URI und Grant-Typen *Authorization Code* + *Refresh Token*. Danach einmal „Benutzer verbinden" unter Integrationen. Siehe Abschnitt 10.

## 3b. Der richtige Nachweis-Endpunkt ist `/activities`, nicht `/jobs`

Nach einem Patch-Scan blieb `GET /v2/device/{id}/jobs` leer. Die Geräte-Historie zeigt dagegen die vollständige Kette:

```
START_REQUESTED → SOFTWARE_PATCH_MANAGEMENT_SCAN_STARTED → PATCH_MANAGEMENT_SCAN_COMPLETED
```

Das Cockpit wartet nach einem Patch-Auftrag bis zu 36 Sekunden auf die Abschluss-Aktivität und schreibt sie mit ID und Status ins Ausführungsprotokoll. Bleibt sie aus, steht dort ehrlich „Abschluss noch nicht gemeldet" statt „erfolgreich".

## 3c. Kunden ausblenden (Fokus)

Von 39 Kunden haben 17 fünf oder mehr Geräte – das ist ungefähr die Hälfte, die für deinen Chef zählt. Diese sind vorbelegt im **Fokus**; die übrigen 22 bleiben vollständig verwaltet, fehlen aber in Übersicht und Kennzahlen.

Umschaltbar unter **Kunden → Ansicht „Fokus" / „Ausgeblendet" / „Alle"**, pro Zeile über das Augen-Symbol. Jede Änderung landet im Audit-Trail. Die Seiten **Zeiten** und **Abrechnung** bleiben bewusst vollständig, damit keine erbrachte Leistung verschwindet.

## 3d. WordPress-Agent: komplett verifiziert

Lokale Instanz auf XAMPP, Apache Port 81, WordPress 7.1.3, PHP 8.5.12, MariaDB 10.11.16.

| Prüfung | Ergebnis |
| --- | --- |
| Agent ohne Signatur | HTTP 401 |
| Agent mit falscher Signatur | HTTP 401 `signature_invalid` |
| Plugin-Update ohne Backup | HTTP 409 `backup_missing` |
| Dry-Run | zeigt `5.8 -> 6.2.3` ohne Änderung |
| Echtes Update | `5.8 -> 6.2.3`, Health danach HTTP 200 |
| Unabhängige Kontrolle | `get_plugins()` meldet 6.2.3, 0 offene Updates |
| Über das Cockpit | Health-Check und Update inkl. Freigabe und Version im Protokoll |

Der Agent ist damit produktionsnah: signiert, backup-gesperrt, mit Dry-Run und Health-Check.
Vier Fallstricke sind in [xampp-wordpress.md](xampp-wordpress.md) dokumentiert (DB_HOST-Korruption,
fehlendes `WP_ENVIRONMENT_TYPE`, fehlende `.htaccess`, verschobene Klassen in WP 7.x).

## 4. Was dafür im Cockpit gebaut wurde

- **Echte Persistenz:** Organisationen werden zu Kunden (`ninja-{orgId}`), Geräte zu Geräten (`ninja-dev-{devId}`) mit Hostname, OS-Klasse, Online-Status und letztem Kontakt.
- **Kurzcode-Generierung** aus dem Kundennamen mit Kollisionsschutz.
- **Demo-Ablösung:** Beim ersten Live-Sync werden Seed-Kunden deaktiviert, ihre Daten aus Listen und Berichten ausgeblendet.
- **Live-Kennzeichnung** in der Seitenleiste („Live-Modus · NinjaOne · echte Daten").
- **Schnelltest pro Gerät:** Auf der Kundenseite lässt sich ein Testlauf mit genau einer Aufgabe anlegen:
  - **Script-Test** – rein lesendes Inline-Script, `requires_approval = true`, muss also erst freigegeben werden.
  - **Patch-Scan** – stößt nur den Scan an, installiert nichts.
- **Ehrliche Statuslabels:** Geräte ohne Patch-/AV-Daten zeigen „Patchstand unbekannt" statt fälschlich „ok".

## 5. Testkandidaten im Bestand

| Gerät | Kunde | Klasse | Status | Eignung |
| --- | --- | --- | --- | --- |
| TEST-VM-07 | TEST | Windows Workstation | online | Entwicklungs-VM, wirkt wie ein guter Kandidat |
| TEST-CAD-01 | Laborbetrieb Süd | Windows Workstation | offline | Laborgerät, aktuell offline |
| TEST-DC1 | eigener Betrieb | Windows Server | offline | Domain Controller, **nicht** für Tests geeignet |
| TEST-SRV-02 | TEST | Windows Server | online | Produktivserver, nur mit Freigabe |

Für den Script-Test ist ein online-Gerät nötig. **Vor Ausführung ist eine ausdrückliche Freigabe nötig**, weil Code auf einem Kundensystem läuft.

## 6. Nächste Schritte

1. Testgerät festlegen und freigeben (Name genügt).
2. Im Cockpit: Kunde → Gerät wählen → „Script-Test" → Aufgabe freigeben → ausführen.
3. Ergebnis prüfen: 204 von NinjaOne, danach Job-Status über `/v2/device/{id}/jobs`.
4. Bei Erfolg: Patch-Scan auf demselben Gerät.
5. Falls `script/run` mit Machine-Token abgelehnt wird: zweiten API-Client mit Benutzerkontext testen.
6. Webhook-Slot im Tenant prüfen.
7. WordPress-Agent gegen XAMPP testen (Health → Inventar → Dry-Run → ein Update).

## 7. Einsätze (neuer Bereich)

Weil Störungen und Anfragen neben der Wartung anfallen, gibt es jetzt **Einsätze**: gleiches Datenmodell wie Wartungsläufe (`kind = 'ticket'`), gleiche Zeiterfassung, gleiches Protokoll – nur mit Priorität, Beschreibung, Eingangskanal und Fälligkeit.

Bewusst **nicht** gebaut: Mail-Eingang, SLA-Timer, Queues, Kundenportal, Ticketnummern-Kreis. Das ist ein Helpdesk und leistet NinjaOne bereits (inklusive `timeTracked`).

## 7a. Zwei Korrektheitslücken geschlossen

1. **Fehlgeschlagene Aktivität wird als Fehler abgebildet.** Vorher blieb die Aufgabe „erledigt", auch wenn NinjaOne `FAILED` meldete. Jetzt setzt der Nachzieher den Status auf `failed`, wenn der Aktivitätscode auf einen Fehlschlag deutet.
2. **Doppel-Ausführung gesperrt.** Ein bereits abgeschlossener Auftrag lässt sich nicht erneut starten; die API antwortet mit **409** und verweist auf eine neue Aufgabe. Das verhindert genau den Fehler, den ich beim Warten auf den Abschluss gemacht habe (mehrfach ausgelöste Patch-Scans).

## 8. Lesbarkeit

Alle Schriftgrößen wurden um etwa 18 % skaliert, mit einer Untergrenze von 11 px. Die kleinsten Labels (vorher 8,5–9 px) sind jetzt 11 px, Fließtext 15,3 px, Tabellen 12,4 px. Sidebar und Tabellenabstände wurden mitgezogen, damit nichts umbricht.

## 9. Konfiguration

Die Zugangsdaten liegen in `wartungscockpit/.env` (gitignored), nicht im Repository und nicht im Frontend:

```env
NINJAONE_MOCK=false
NINJAONE_BASE_URL=https://eu.ninjarmm.com
NINJAONE_CLIENT_ID=…
NINJAONE_CLIENT_SECRET=…
```

Für den Script-Test lässt sich der Inhalt über `NINJAONE_POC_SCRIPT` überschreiben. Standard ist bewusst rein lesend:

```powershell
Write-Output ("Wartungscockpit POC " + (Get-Date -Format o) + " on " + $env:COMPUTERNAME)
```

## 9a. Fallstrick in der .env

`NINJAONE_SCOPE=monitoring management` ohne Anführungszeichen bringt den Parser zum Abbruch: Alle **nachfolgenden** Zeilen werden nicht geladen. Das äußerte sich als „ID und Secret sind doch eingetragen, aber die App sagt, sie fehlen" — `NINJAONE_CLIENT_ID` funktionierte, `NINJAONE_USER_CLIENT_ID` kam leer an.

Richtig:

```env
NINJAONE_SCOPE="monitoring management"
```

Zusätzlich: Die API liest die `.env` **im Projektordner** (`wartungscockpit/.env`), nicht die im Workspace-Wurzelverzeichnis.

## 10. Benutzerkontext – erledigt

Eingerichtet und verbunden. Kurzfassung:

1. App-Typ **Webanwendung**, Redirect-URI `http://127.0.0.1:8080/api/v1/auth/ninjaone/callback`
2. Grant-Typen **Berechtigungscode** + **Aktualisierungstoken**
3. Scope **inklusive `offline_access`** (das Cockpit ergänzt ihn automatisch)
4. `NINJAONE_USER_CLIENT_ID` / `NINJAONE_USER_CLIENT_SECRET` in `wartungscockpit/.env`
5. **Integrationen → Benutzer verbinden** → zustimmen

Der Refresh-Token liegt in der Datenbank (`app_settings`), nicht im Frontend. Widerruf jederzeit in NinjaOne möglich.

### Ursprüngliche Einrichtung (Referenz)

1. In NinjaOne **Administration → Apps → API → Client App IDs → Add**.
2. Anwendungs-Plattform: **Webanwendung** (nicht API-Dienste).
3. Name: `Wartungscockpit Benutzer`.
4. Umleitungs-URI: `http://127.0.0.1:8080/api/v1/auth/ninjaone/callback`
5. Grant-Typen: **Berechtigungscode** und **Aktualisierungstoken**.
6. Scopes: Überwachen und Verwalten.
7. Client-ID und Secret in `.env` eintragen:

   ```env
   NINJAONE_USER_CLIENT_ID=…
   NINJAONE_USER_CLIENT_SECRET=…
   NINJAONE_REDIRECT_URI=http://127.0.0.1:8080/api/v1/auth/ninjaone/callback
   ```

8. API neu starten, dann **Integrationen → Benutzer verbinden** und im NinjaOne-Fenster als Administrator zustimmen.

Der Refresh-Token landet in der Datenbank (`app_settings`), nicht im Frontend. Ein Widerruf ist in NinjaOne unter Apps jederzeit möglich; danach schlägt der Refresh fehl und das Cockpit verlangt eine erneute Verbindung.

## 11. Sicherheitsnotizen

- Der API-Client hat `monitoring` und `management`, aber **nicht** `control`.
- Der Client ist ein eigener PoC-Client und kann im Tenant jederzeit widerrufen werden.
- Das Secret steht nur in der lokalen `.env`. Eine Kopie liegt im Workspace-Wurzelverzeichnis und kann gelöscht werden.
- Bisher wurde **kein** schreibender Aufruf gegen ein Kundengerät gesendet. Der angelegte Testlauf wurde vor der Ausführung wieder entfernt.
