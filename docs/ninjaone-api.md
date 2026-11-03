# NinjaOne Public API v2 – Arbeitsreferenz

Diese Referenz dokumentiert die für das Wartungscockpit relevanten Endpunkte und die Punkte, die vor einem Live-Betrieb zwingend getestet werden müssen. Maßgeblich ist immer die OpenAPI-Spezifikation des eigenen Tenants:

- [Public API Operations](https://www.ninjaone.com/docs/application-programming-interface-api/public-api-operations/)
- [OpenAPI-Spezifikation](https://app.ninjarmm.com/apidocs/NinjaRMM-API-v2.json)
- [Webhooks](https://app.ninjarmm.com/apidocs-beta/core-resources/articles/webhooks/overview)

## 1. Authentifizierung

OAuth 2.0, Client Credentials:

```bash
export NINJA_URL="https://eu.ninjarmm.com"
export NINJA_ID="…"
export NINJA_SECRET="…"

curl -s -X POST "$NINJA_URL/oauth/token" \
  -H "Content-Type: application/x-www-form-urlencoded" \
  -d "grant_type=client_credentials" \
  -d "client_id=$NINJA_ID" \
  -d "client_secret=$NINJA_SECRET" \
  -d "scope=monitoring management"
```

Antwort:

```json
{
  "access_token": "…",
  "token_type": "Bearer",
  "expires_in": 3600,
  "scope": "monitoring management"
}
```

- **Scopes:** `monitoring` für lesende Daten und Org-Struktur, `management` für Änderungen, Scripts und Patch-Aktionen. `control` wird für Remote Control benötigt und vom Cockpit **nicht** verwendet.
- **Token-Cache:** 60 Sekunden vor Ablauf erneuern. Bei 401 Token verwerfen und einmal neu holen.
- **Basis-URLs:** `app.ninjarmm.com` (US), `us2.ninjarmm.com`, `eu.ninjarmm.com`, `oc.ninjarmm.com`, `ca.ninjarmm.com`. Die URL ist in der App konfigurierbar.

## 2. Endpunktmatrix

| Bereich | Methode | Pfad | Zweck | PoC-Status |
| --- | --- | --- | --- | --- |
| Auth | POST | `/oauth/token` | Client-Credentials-Token | implementiert |
| Kunden | GET | `/v2/organizations` | Organisationen lesen | implementiert |
| Geräte | GET | `/v2/organization/{id}/devices` | Geräte einer Organisation | implementiert |
| Geräte | GET | `/v2/device/{id}` | Einzelnes Gerät | Client vorhanden |
| Patches | POST | `/v2/device/{id}/patch/os/scan` | OS-Patch-Scan anstoßen | implementiert |
| Patches | POST | `/v2/device/{id}/patch/os/apply` | OS-Patches installieren | implementiert |
| Patches | POST | `/v2/device/{id}/patch/software/scan` | Software-Patch-Scan | implementiert |
| Patches | POST | `/v2/device/{id}/patch/software/apply` | Software-Patches installieren | implementiert |
| Gerät | POST | `/v2/device/{id}/reboot/{NORMAL\|FORCED}` | Neustart mit Begründung | implementiert |
| Gerät | PUT | `/v2/device/{id}/maintenance` | Wartungsfenster setzen | implementiert |
| Gerät | DELETE | `/v2/device/{id}/maintenance` | Wartungsfenster beenden | implementiert |
| Scripts | POST | `/v2/device/{id}/script/run` | Script ausführen | implementiert, Live-Test offen |
| Jobs | GET | `/v2/device/{id}/jobs` | Job-Status lesen | Client vorhanden |
| Tickets | GET | `/v2/ticketing/ticket/{id}/log-entry` | Log-/Zeiteinträge lesen | nicht implementiert |
| Webhooks | PUT | `/v2/webhook` | Webhook konfigurieren | nicht implementiert |

Die genauen Request-Schemas können je nach Tenant-Version abweichen. Vor dem Live-Betrieb gegen die eigene OpenAPI-Spezifikation prüfen, insbesondere bei Patch- und Script-Endpunkten.

### Was die API nicht kann

- **Patches genehmigen oder ablehnen.** In der OpenAPI-Spezifikation (2.0.9-draft) existieren
  genau vier schreibende Patch-Endpunkte, alle `scan` oder `apply`. `apply` nimmt **keinen
  Body** an – man kann also nicht einmal einzelne Patches auswählen, es wird immer alles
  eingespielt, was die Policy als genehmigt ansieht. `Approve/Reject` gibt es nur für
  **Geräte** (`/v2/devices/approval/{mode}`) und **Rechnungen** (`/v2/billing/invoices/approve`).
  Die Freigabe ist fachlich Teil der Patch-Policy, nicht des Patches.
- **Virenschutz-Scan anstoßen.** Es gibt keinen Endpunkt dafür; die Geräte-Historie zeigt
  AV-Scans nur, wenn sie über eine Policy oder den Agenten laufen.
- **Remote Control starten.** Die Splashtop-Session wird über ein privates Token im Frontend
  erzeugt, nicht über die Public API. Deshalb der Deep-Link in die Geräteseite.

## 3. Typische Aufrufe

### Organisationen und Geräte

```bash
TOKEN="…"

curl -s -H "Authorization: Bearer $TOKEN" \
  "$NINJA_URL/v2/organizations"

curl -s -H "Authorization: Bearer $TOKEN" \
  "$NINJA_URL/v2/organization/101/devices"
```

### Patch-Scan und Installation

```bash
# Scan
curl -s -o /dev/null -w "%{http_code}\n" -X POST \
  -H "Authorization: Bearer $TOKEN" \
  "$NINJA_URL/v2/device/101001/patch/os/scan"

# Installation
curl -s -o /dev/null -w "%{http_code}\n" -X POST \
  -H "Authorization: Bearer $TOKEN" \
  "$NINJA_URL/v2/device/101001/patch/os/apply"
```

- Erfolg ist typischerweise **204 No Content**.
- **400** bedeutet oft, dass das Gerät für die Aktion nicht geeignet ist (z. B. kein ausstehendes Patch).
- Die Aktion ist **asynchron**. Der eigentliche Fortschritt steht im Geräte-Job.

Wichtig für die Auswertung: **Ein Fehlschlag kommt als abgeschlossene Aktivität zurück.**
NinjaOne meldet z. B. `PATCH_MANAGEMENT_SCAN_COMPLETED` mit dem Text „Der OS-Patch-Scan konnte
nicht abgeschlossen werden. Es ist nicht genügend Speicherplatz vorhanden." Wer nur auf den
Statuscode `COMPLETED` schaut, zeigt „OK", obwohl nichts passiert ist. Deshalb prüft das Cockpit
zusätzlich den Meldungstext (`konnte nicht`, `nicht abgeschlossen`, `failed to`, `could not be completed`).

### Neustart und Wartungsfenster

```bash
# Neustart: Modus steckt im Pfad, Begründung im Body
curl -s -o /dev/null -w "%{http_code}\n" -X POST \
  -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" \
  -d '{"reason": "Wartung über Wartungscockpit"}' \
  "$NINJA_URL/v2/device/101001/reboot/NORMAL"

# Wartungsfenster: start/end als Epoch-Millisekunden
curl -s -o /dev/null -w "%{http_code}\n" -X PUT \
  -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" \
  -d '{"start": 1761667200000, "end": 1761670800000, "disabledFeatures": ["ALERTS"]}' \
  "$NINJA_URL/v2/device/101001/maintenance"

# vorzeitig beenden
curl -s -o /dev/null -w "%{http_code}\n" -X DELETE \
  -H "Authorization: Bearer $TOKEN" \
  "$NINJA_URL/v2/device/101001/maintenance"
```

`disabledFeatures` kennt `ALERTS`, `PATCHING`, `AVSCANS` und `TASKS`. Das Cockpit setzt nur
`ALERTS` – Patch-Läufe und AV-Scans sollen während einer Wartung weiterlaufen.

**Es gibt kein GET auf `/maintenance`.** Ob ein Fenster gesetzt ist, lässt sich nicht abfragen;
das Cockpit merkt sich das Ende deshalb selbst in `devices.maintenance_until`.

### Script ausführen

```bash
curl -s -X POST \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"scriptId": 123, "runAs": "SYSTEM"}' \
  "$NINJA_URL/v2/device/101001/script/run"
```

Alternativ `scriptContent` für ein Inline-Script. Für den ersten Test ein harmloses Script verwenden, zum Beispiel eine Ausgabe der aktuellen Zeit oder `whoami`.

### Jobs prüfen

```bash
curl -s -H "Authorization: Bearer $TOKEN" \
  "$NINJA_URL/v2/device/101001/jobs"
```

Im PoC wird nach dem Absenden nur der Eingang bestätigt. Phase 1 pollt diesen Endpunkt, bis der Job beendet ist, und übernimmt Ergebnis und Dauer.

## 4. Webhooks

```bash
curl -s -X PUT \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "url": "https://cockpit.example.de/api/v1/webhooks/ninjaone",
    "activities": {
      "CONDITION": "*",
      "ACTIONSET": "*",
      "DEVICE": "*"
    },
    "headers": {
      "Authorization": "Bearer ein-eigenes-token"
    },
    "secret": "optional-fuer-hmac"
  }' \
  "$NINJA_URL/v2/webhook"
```

**Kritisch:** Es gibt genau **eine** Webhook-Konfiguration pro Tenant. Vor dem Bau eines Empfängers prüfen:

- Ist der Webhook bereits durch PSA, SIEM oder ein anderes Tool belegt?
- Falls ja: Polling-Fallback einplanen oder den bestehenden Empfänger ablösen.
- Der Empfänger muss die Signatur/das Secret prüfen und idempotent verarbeiten.
- Webhook-Ausfälle müssen sichtbar sein (NinjaOne-Systemaktivität „Webhook failure“).

## 5. Rate-Limits, Pagination, Fehler

- **Rate-Limits** sind nicht öffentlich beziffert. Der Client behandelt `429` mit `Retry-After` oder exponentiellem Backoff (3 Versuche).
- **Pagination** ist cursor-basiert (`pageSize` plus `cursor`/`after`). Große Tenants liefern nicht alles in einer Antwort.
- **Fehler:** 400 (ungültige Anfrage/Gerät nicht geeignet), 401 (Token), 403 (Scope/Recht), 404, 429, 5xx.
- **Timeouts:** 30 Sekunden pro Anfrage. Lange Patch- oder Script-Läufe laufen asynchron weiter und dürfen die HTTP-Anfrage nicht blockieren.

## 6. Live-Test-Checkliste

Vor Phase 1 mit einem echten Tenant abhaken:

- [ ] API-App als „API Services (machine-to-machine)“ angelegt
- [ ] Client-ID und Secret im Secret Store, nicht im Repository
- [ ] `GET /v2/organizations` liefert die erwarteten Kunden
- [ ] `GET /v2/organization/{id}/devices` liefert Geräte mit stabilen IDs
- [ ] `POST …/patch/os/scan` gegen ein Testgerät liefert 204
- [ ] `GET …/jobs` zeigt den Scan-Job und dessen Ergebnis
- [ ] `POST …/patch/os/apply` gegen ein Testgerät mit offenen Patches
- [ ] `POST …/script/run` mit **Machine-Token** erfolgreich
- [ ] Falls Script-Run scheitert: Benutzerkontext-Token oder anderer App-Typ testen
- [ ] `POST …/script/run` mit Inline-Script und `runAs=SYSTEM`
- [ ] Webhook-Slot im Tenant prüfen (belegt/frei)
- [ ] 429-Verhalten mit kleinen Parallelaufrufen beobachten
- [ ] Patch- und Script-Rechte der API-App dokumentieren
- [ ] Verhalten bei offline gegangenem Gerät prüfen
- [ ] Zeitstempel und Zeitzonen der Job-Rückmeldungen prüfen

## 7. Abbildung im Code

- Client und Mock-Fixtures: `apps/api/src/ninjaone/mod.rs`
- Ausführung und Job-Fortschritt: `apps/api/src/executors.rs`
- Sync-Endpunkt: `apps/api/src/routes/mod.rs` (`sync_ninjaone`)
- Konfiguration: `.env.example`, `apps/api/src/config.rs`

Der Mock-Modus ist standardmäßig aktiv, solange keine Client-ID und kein Secret gesetzt sind. Für den Live-Betrieb:

```env
NINJAONE_MOCK=false
NINJAONE_BASE_URL=https://eu.ninjarmm.com
NINJAONE_CLIENT_ID=…
NINJAONE_CLIENT_SECRET=…
```

Nach dem Setzen die API neu starten.

## 8. Noch nicht implementiert

- Webhook-Empfänger und Signaturprüfung
- Polling bis zur endgültigen Job-Beendigung
- Ticket-Sync und `timeTracked`
- Pagination über alle Seiten
- Device-Filter (`df`) und Berichts-Endpunkte
- Antivirus-Aktionen (die API bietet keinen Scan-Endpunkt; Status lesen ist möglich)
- Patch-Freigabe über die API (gibt es nicht, bleibt in der NinjaOne-Policy)
- Script-Auswahl aus der NinjaOne-Bibliothek in der Oberfläche (Client kann es, Route fehlt)
