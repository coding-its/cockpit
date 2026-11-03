# XAMPP & WordPress lokal – Einrichtung und Ergebnis

Stand: 26. Oktober 2026 · **eingerichtet und getestet**

## Ergebnis

Die lokale Testinstanz läuft und ist mit dem Cockpit verbunden:

| Punkt | Wert |
| --- | --- |
| URL | http://localhost:81/wordpress |
| WordPress | 7.1.3 (de_DE) |
| PHP | 8.5.12 |
| MariaDB | 10.11.16, Port 3306 |
| Apache | 2.4.65, Port 81 |
| Agent-Benutzer | `wartung-cockpit` (Administrator, eigenes Application Password) |
| Mu-Plugin | `wp-content/mu-plugins/wartungscockpit-agent.php` |
| Backup | `.tmp/backup/wordpress-files.tar.gz` (36 MB) + `wordpress-db.sql` (120 KB) |

Verifiziert über das Cockpit:

- Health-Check: „HTTP 200, WordPress 7.1.3, PHP 8.5.12, 0 Plugin-Updates offen"
- Plugin-Update: Contact Form 7 **5.8 → 6.2.3**, Health-Check danach OK
- Ohne Freigabe: HTTP 409 „als risikoreich markiert"
- Ohne Backup-Bestätigung: Abbruch mit klarer Meldung

---

## Ursprüngliche Diagnose (Archiv)

**Gelöst durch Portwechsel:** Apache läuft jetzt auf **Port 81**, IIS bleibt unangetastet. Das war die bessere Variante – kein Eingriff in Windows-Dienste.

**Ein Neuinstallieren von XAMPP hätte nichts geholfen.** Der Grund lag außerhalb von XAMPP: Der **IIS lief und belegte Port 80**. Eine frische Installation trifft auf dieselbe Belegung.

## Befunde

| Prüfung | Ergebnis |
| --- | --- |
| XAMPP vorhanden | `C:\xampp` |
| Apache-Dienst registriert | nein (XAMPP startet `httpd.exe` normalerweise direkt) |
| IIS-Dienst `W3SVC` | **läuft** |
| Port 80 | **belegt** (Zugriff verweigert beim Bind-Versuch → typisch für `http.sys`/IIS) |
| Port 443 | frei |
| Port 3306 (MySQL) | belegt, `mysqld.exe` läuft |
| Port 8080 | belegt durch das Wartungscockpit |
| PHP | `C:\xampp\php\php.exe`, PHP 8.2.12 |
| Inhalt `C:\inetpub\wwwroot` | nur `iisstart.htm` und `iisstart.png` → Standardseite, kein Anwendungscode |
| Apache-Fehlerprotokoll | letzter erfolgreicher Start im Januar 2026, seither kein Eintrag → Apache wurde nicht mehr gestartet, seit IIS übernommen hat |

`C:\xampp\htdocs` enthält noch **kein** `wordpress`-Verzeichnis.

## Lösung A (nicht gewählt): IIS stoppen

Da in `wwwroot` nur die IIS-Begrüßungsseite liegt, wird IIS auf diesem Rechner offenbar nicht gebraucht.

Als Administrator (PowerShell oder Eingabeaufforderung):

```cmd
net stop W3SVC
sc config W3SVC start= demand
```

Danach XAMPP Control Panel → **Apache Start**. Prüfen: [http://localhost](http://localhost) zeigt die XAMPP-Startseite.

Falls IIS doch irgendwo gebraucht wird: `sc config W3SVC start= auto` und `net start W3SVC` stellt den Ausgangszustand wieder her.

## Lösung B: Apache auf einen anderen Port legen

Nur wenn IIS bleiben muss. In `C:\xampp\apache\conf\httpd.conf`:

```apache
Listen 80        →  Listen 8081
ServerName localhost:80  →  ServerName localhost:8081
```

Ebenso in `httpd-ssl.conf` ggf. `Listen 443` prüfen. Danach Apache starten und [http://localhost:8081](http://localhost:8081) aufrufen.

**Nicht 8080 nehmen** – der Port ist vom Wartungscockpit belegt.

Wichtig für WordPress: Die `WP_HOME`/`WP_SITEURL`-Werte müssen dann `http://localhost:8081/wordpress` enthalten, sonst leitet WordPress falsch um.

## WordPress installieren

1. [https://de.wordpress.org/download/](https://de.wordpress.org/download/) als ZIP laden.
2. Nach `C:\xampp\htdocs\wordpress` entpacken.
3. In phpMyAdmin eine Datenbank `wordpress` mit `utf8mb4_general_ci` anlegen.
4. `wp-config-sample.php` → `wp-config.php` kopieren, `DB_NAME`, `DB_USER` (`root`), `DB_PASSWORD` (leer) eintragen.
5. In `wp-config.php` ergänzen:

   ```php
   define('WP_ENVIRONMENT_TYPE', 'local');
   define('WC_MAINT_SHARED_SECRET', 'ein-langes-zufaelliges-secret');
   ```

6. `http://localhost/wordpress/wp-admin/install.php` aufrufen und die Installation abschließen.

**Application Passwords** erscheinen nur, wenn WordPress die Seite als „lokal" oder HTTPS erkennt. `WP_ENVIRONMENT_TYPE = local` setzt das zuverlässig, auch hinter einem anderen Port.

## Agent installieren

```cmd
mkdir C:\xampp\htdocs\wordpress\wp-content\mu-plugins
copy packages\wp-agent\wartungscockpit-agent.php C:\xampp\htdocs\wordpress\wp-content\mu-plugins\
```

Danach im WordPress-Backend unter **Benutzer → Profil** ein Application Password erzeugen und den signierten Testclient benutzen:

```bash
export WP_URL="http://localhost:8081"      # oder http://localhost
export WP_USER="wartung-cockpit"
export WP_APP_PASSWORD="xxxx xxxx xxxx xxxx xxxx xxxx"
export WP_AGENT_SECRET="ein-langes-zufaelliges-secret"

cd packages/wp-agent
node test-request.mjs health
node test-request.mjs inventory
```

## Fallback ohne Apache

Falls IIS bleiben muss und kein Port frei ist: PHP kann einen eigenen Server starten.

```cmd
C:\xampp\php\php.exe -S localhost:8081 -t C:\xampp\htdocs\wordpress
```

Das reicht für REST-Tests (`/wp-json/maintenance/v1/health`), ist aber **kein** Produktivbetrieb: Pretty Permalinks und manche Admin-Aufrufe brauchen einen echten Webserver.

## Wenn Apache weiter nicht startet

1. **Port prüfen:** `netstat -ano | findstr :80` – die PID in der letzten Spalte verrät den Verursacher.
2. **Fehlerprotokoll:** `C:\xampp\apache\logs\error.log`, letzte Zeilen nach dem Startversuch.
3. **Konfiguration prüfen:** `C:\xampp\apache\bin\httpd.exe -t` zeigt Syntaxfehler im Klartext.
4. **Visual C++ Redistributable** (x64, 2015–2022) muss installiert sein; sonst beendet sich `httpd.exe` kommentarlos.
5. **Virenscanner** blockiert manchmal `httpd.exe` oder den Port.

---

## Was bei der Einrichtung zusätzlich auffiel

### 1. DB_HOST war korrupt

In `wp-config-sample.php` stand `DB_HOST` als `localhost:81` – der **Web**-Port statt des
Datenbank-Ports. Ursache ist ein Suchen-und-Ersetzen über die ganze WordPress-Installation
(`localhost` nach `localhost:81`). WordPress versuchte dann, MariaDB auf Port 81 zu erreichen,
und lief in einen Timeout:

```
Maximum execution time of 120 seconds exceeded ... mysqli_real_connect(..., 81, ...)
```

Richtig ist `define('DB_HOST', 'localhost');` – Port 3306 ist der Standard.

### 2. WP_ENVIRONMENT_TYPE ist Pflicht

Ohne `define('WP_ENVIRONMENT_TYPE', 'local');` bietet WordPress keine Application
Passwords über HTTP an. Der Abschnitt fehlt dann still im Benutzerprofil.

### 3. .htaccess fehlte

Nach einer CLI-Installation schreibt WordPress keine Rewrite-Regeln. Ohne
`.htaccess` antwortet `wp-json` mit 404. Zusätzlich muss die Authorization-Weiterleitung
drinstehen, sonst kommen Application Passwords unter Apache nicht an:

```apache
RewriteRule .* - [E=HTTP_AUTHORIZATION:%{HTTP:Authorization}]
```

### 4. WordPress 7.x verschiebt Klassen

`class-wp-application-passwords.php` liegt jetzt unter `wp-includes/`, nicht mehr unter
`wp-admin/includes/`. Ältere Anleitungen greifen ins Leere.

### 5. Kein ZipArchive im XAMPP-PHP

Backups per PHP-ZIP scheitern. `tar` aus Git Bash funktioniert:

```bash
cd /c/xampp/htdocs && tar -czf backup.tar.gz wordpress
C:/xampp/mysql/bin/mysqldump.exe -u root --databases wordpress > backup.sql
```

### 6. Umlaute in curl-Bodies

In Git Bash brechen Umlaute im `-d`-Parameter die JSON-Kodierung
(`invalid unicode code point`). Für API-Tests ASCII verwenden oder die Nutzlast in eine
Datei schreiben.

## Nachbau in Kurzform

1. WordPress nach `C:\xampp\htdocs\wordpress` entpacken.
2. `wp-config.php` anlegen: `DB_HOST=localhost`, `WP_ENVIRONMENT_TYPE=local`,
   `WP_HOME`/`WP_SITEURL` auf `http://localhost:81/wordpress`, `WC_MAINT_SHARED_SECRET` setzen.
3. `.htaccess` mit `RewriteBase /wordpress/` und der `HTTP_AUTHORIZATION`-Regel anlegen.
4. Installation und Agent-Benutzer über die Skripte in `packages/wp-agent/local-tools/`.
5. Mu-Plugin nach `wp-content/mu-plugins/` kopieren.
6. Agent im Cockpit hinterlegen:

```
PATCH /api/v1/sites/{site-id}/agent
{
  "agent_user": "wartung-cockpit",
  "agent_password": "…",
  "agent_secret": "…",
  "backup_confirmed": true,
  "backup_reference": "Backup vom …"
}
```

7. Health-Check als Aufgabe anlegen (`template: "wp-health"`) und ausführen.
