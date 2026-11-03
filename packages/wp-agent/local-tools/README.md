# Lokale Werkzeuge für den WordPress-Test

Hilfsskripte für XAMPP. Sie gehören **nicht** ins Webroot – dort wären sie
per HTTP erreichbar und würden Passwörter preisgeben.

Aufruf immer mit dem XAMPP-PHP und dem Pfad zur WordPress-Installation:

```cmd
cd C:\xampp\htdocs\wordpress
C:\xampp\php\php.exe C:\pfad\zu\local-tools\wp-install-headless.php
```

## Skripte

| Datei | Zweck |
| --- | --- |
| `wp-install-headless.php` | Führt den WordPress-Installer ohne Browser aus, setzt Permalinks |
| `wp-create-agent-user.php` | Legt `wartung-cockpit` an und erzeugt ein Application Password |
| `wp-install-plugin.php <zip>` | Installiert ein Plugin aus einer ZIP und aktualisiert die Update-Transients |

## Wichtig

- `wp-install-headless.php` setzt das Admin-Passwort hart auf `WartungCockpit!2026`.
  Nur für lokale Testinstanzen.
- `wp-create-agent-user.php` gibt das Application Password **einmalig** auf der
  Konsole aus. Danach steht es nur noch in der Datenbank.
- Beide Skripte setzen `$_SERVER`-Variablen voraus, damit `wp-load.php` außerhalb
  des Webservers funktioniert.
