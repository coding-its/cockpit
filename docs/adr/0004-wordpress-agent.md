# ADR 0004 – WordPress-Agent als Mu-Plugin, WP-CLI als Fallback

**Status:** entschieden · Oktober 2026

## Kontext

Die WordPress-Core-REST-API kann Plugins auflisten, aktivieren und installieren, aber **keine Plugin-Updates auf eine neue Version** einspielen. Ein Wartungscockpit muss aber genau das können. Gleichzeitig sind Hoster und Zugriffswege sehr unterschiedlich: REST kann blockiert sein, Loopback kann fehlschlagen, SSH ist nicht überall verfügbar.

## Entscheidung

- Ein eigenes Mu-Plugin (`packages/wp-agent`) stellt signierte REST-Endpunkte bereit:
  - `GET /health`
  - `GET /inventory`
  - `POST /updates/plugins`
  - `POST /updates/core`
  - `GET /log`
- Authentifizierung: WordPress Application Password + HMAC-SHA256 über Zeitstempel und Raw Body + optionale IP-Allowlist + Rate-Limit.
- Updates nur mit `backup_confirmed=true`. Der Agent erstellt selbst kein Backup.
- Dry-Run ist der sichere Standard; die Oberfläche zeigt zuerst die geplanten Versionen.
- Nach jedem Update: Health-Check per HTTP und Fatal-Error-Logging über Shutdown-Handler.
- WP-CLI über SSH als Fallback, ausschließlich mit fest verdrahteten Befehlen.
- Kein automatisches Rollback-Versprechen ohne Snapshot.

## Konsequenzen

**Positiv:** Funktioniert auf jedem WordPress mit REST und HTTPS, kein SSH-Zugang nötig. Klare Rechteprüfung über WordPress-Capabilities. Updates sind auditierbar und backup-gesperrt.

**Negativ:** Zusätzliches Plugin, das gepflegt werden muss. Manche Hoster blockieren REST oder Loopback. Application Passwords setzen HTTPS voraus. Der Agent muss bei WordPress-/PHP-Updates kompatibel bleiben.

**Alternativen:** MainWP wäre eine fertige Alternative, wenn es bereits im Einsatz ist. Reines WP-CLI ist mächtig, aber credential- und hostingabhängig. Ein generischer „WP-Admin-Login-Bot“ ist ausgeschlossen: fragil, unsicher, nicht auditierbar.
