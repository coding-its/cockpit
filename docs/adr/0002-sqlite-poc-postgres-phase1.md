# ADR 0002 – SQLite im PoC, PostgreSQL ab Phase 1

**Status:** entschieden · Oktober 2026

## Kontext

Der PoC soll ohne Docker, ohne Datenbankserver und ohne Setup lauffähig sein. In der Entwicklungsumgebung ist kein Docker verfügbar. Gleichzeitig ist klar, dass ein Mehrbenutzerbetrieb mit parallelen Timern, Jobs und Reports eine robuste Datenbank braucht.

## Entscheidung

- **PoC:** SQLite-Datei über SQLx, WAL-Modus, Foreign Keys aktiviert. Die Datei liegt unter `data/cockpit.db`.
- **Phase 1:** PostgreSQL 16 über Docker Compose oder verwalteten Dienst.
- Das Schema verwendet TEXT-IDs, ISO-8601-Zeitstempel als TEXT und keine SQLite-spezifischen Typen. Aggregate und Filter sind so geschrieben, dass sie sich weitgehend übertragen lassen.
- Migrationen liegen als SQL-Dateien unter `apps/api/migrations` und werden von SQLx eingebettet.

## Konsequenzen

**Positiv:** `cargo run` genügt für den Einstieg. Demo-Daten sind reproduzierbar. Kein Infrastruktur-Setup für Interessenten.

**Negativ:** SQLite hat nur einen Schreiber. Parallele Timer, Job-Updates und Reports können unter Last blockieren. `substr(started_at, 1, 7)` für Monatsfilter ist ein PoC-Komfort, in PostgreSQL wird daraus `date_trunc('month', started_at)`.

**Migration:** In Phase 1 werden die Queries auf PostgreSQL-Typen (`timestamptz`, `uuid`, `numeric`) und `sqlx::query!` mit Offline-Daten umgestellt. Die Seed-Logik bleibt erhalten, wird aber in einen expliziten Demo-Seeder verschoben.

**Alternativen:** SQLite dauerhaft wäre für ein kleines Team mit wenigen gleichzeitigen Nutzern machbar, aber Backup, Reporting und spätere Mandantenfähigkeit sind mit PostgreSQL sauberer. Ein direkter Start mit PostgreSQL hätte den PoC in dieser Umgebung nicht lauffähig gemacht.
