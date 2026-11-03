# WordPress Maintenance Agent

Mu-Plugin für das Wartungscockpit. Es stellt signierte REST-Endpunkte für Inventar,
Plugin-Updates, Core-Updates und Health-Checks bereit.

## Installation

1. Datei `wartungscockpit-agent.php` nach `wp-content/mu-plugins/` kopieren.
   Den Ordner `mu-plugins` bei Bedarf anlegen.
2. In `wp-config.php` ein gemeinsames Secret hinterlegen:

   ```php
   define('WC_MAINT_SHARED_SECRET', 'hier-ein-langes-zufaelliges-secret');
   ```

3. Einen Administrator-Benutzer anlegen und ein **Application Password** erzeugen
   (Benutzerprofil → Application Passwords). Das normale Passwort wird nicht verwendet.
4. HTTPS erzwingen. Application Passwords funktionieren nur über HTTPS (oder lokal).
5. Optional die IP des Cockpit-Servers freigeben:

   ```php
   add_filter('wc_maintenance_allowed_ips', function () {
       return array('203.0.113.10');
   });
   ```

## Endpunkte

Basis: `https://example.de/wp-json/maintenance/v1`

| Methode | Pfad | Zweck | Capability |
| --- | --- | --- | --- |
| GET | `/health` | Site-, Core-, PHP-, Plugin- und Update-Status | `manage_options` |
| GET | `/inventory?refresh=true` | Plugin-/Theme-Inventar und Updates | `manage_options` |
| POST | `/updates/plugins` | Plugin-Updates, standardmäßig Dry-Run | `update_plugins` |
| POST | `/updates/core` | Core-Update, standardmäßig Dry-Run | `update_core` |
| GET | `/log` | Letzte Agent-Aktionen und Fehler | `manage_options` |

## Authentifizierung

Jede Anfrage braucht:

- HTTP Basic Auth mit Benutzername und Application Password,
- `X-Maintenance-Timestamp` (Unix-Sekunden, maximal 5 Minuten alt),
- `X-Maintenance-Signature`: HMAC-SHA256 über `<timestamp>.<raw body>` mit dem Shared Secret.

Der Testclient signiert automatisch:

```bash
export WP_URL="https://www.example.de"
export WP_USER="wartung"
export WP_APP_PASSWORD="xxxx xxxx xxxx xxxx xxxx xxxx"
export WP_AGENT_SECRET="…"

node test-request.mjs health
node test-request.mjs inventory
node test-request.mjs update-plugins                 # Dry-Run
node test-request.mjs update-plugins --apply --backup
node test-request.mjs update-core --apply --backup
```

## Update-Sicherheit

- **Kein Update ohne `backup_confirmed: true`.** Der Agent erstellt selbst kein Backup.
  Das Cockpit muss vorher ein verifiziertes Backup nachweisen (Backup-Plugin, Host-Snapshot
  oder externer Job).
- **Dry-Run zuerst.** `dry_run: true` liefert die geplanten Versionen, ohne etwas zu ändern.
- **Health-Check danach.** Der Agent ruft die Startseite per HTTP auf und sucht nach
  Fatal-Error-Markern. Ein fehlgeschlagener Loopback ist ein „unbekannt“, kein Erfolg.
- **Fatal-Error-Log.** Ein Shutdown-Handler protokolliert fatale Fehler während eines Updates
  im Agent-Log.
- **Kein automatisches Rollback.** Ohne Dateisystem-Snapshot ist ein Rollback nicht möglich.
  Deshalb: Backup, Dry-Run, Freigabe, Health-Check und klare Fehlermeldung statt stiller Automatik.
- **Replay-Schutz.** Der Zeitstempel begrenzt die Gültigkeit auf fünf Minuten. Ein Nonce-Cache
  gegen die Wiederverwendung einer Signatur innerhalb dieses Fensters ist Phase-1-Härtung.
- **FS_METHOD direct.** Der Agent nutzt direkten Dateisystemzugriff. Der Webserver-Benutzer
  muss Schreibrechte auf `wp-content` haben; sonst schlägt das Update sauber fehl.

## WP-CLI-Fallback

Für Hosts ohne REST-Zugriff oder mit blockiertem Loopback:

```bash
wp plugin list --format=json
wp plugin update --all
wp core version
wp core update
wp core update-db
```

Das Cockpit führt ausschließlich diese festen Befehle aus, niemals frei eingegebenen Shell-Code.
SSH-Zugangsdaten werden verschlüsselt gespeichert und pro Site widerrufen.

## Grenzen des Prototyps

- Kein Rollback und kein Backup-Ersatz.
- Kein Multi-Site-Netzwerk-Support in Version 0.1.
- Kein Vendor-Patch-Management für Premium-Plugins ohne WordPress.org-Updatequelle.
- Keine Vulnerability-Datenbank; diese kommt über WPScan/Patchstack in Phase 2.
