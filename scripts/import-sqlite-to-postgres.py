#!/usr/bin/env python3
"""Übernimmt die Daten aus der alten SQLite-Datei in PostgreSQL.

Einmaliger Umzug: Die Stunden, Kunden und Geräte liegen bisher in
`data/cockpit.db`. Dieses Skript liest sie und schreibt sie in die
PostgreSQL-Datenbank. Mehrfaches Ausführen ist unschädlich – vorhandene
Zeilen werden übersprungen (`ON CONFLICT DO NOTHING`).

Aufruf (im Projektordner):

    python scripts/import-sqlite-to-postgres.py --sqlite data/cockpit.db --out data/import.sql
    "C:\\Program Files\\PostgreSQL\\18\\bin\\psql.exe" -h 127.0.0.1 -p 5432 \
        -U cockpit -d wartungscockpit -v ON_ERROR_STOP=1 -f data/import.sql

Das Skript schreibt bewusst eine SQL-Datei, statt selbst zu verbinden:
So braucht es keinen Datenbanktreiber, und man kann vor dem Einspielen
hineinschauen.
"""

import argparse
import sqlite3
import sys
from pathlib import Path

# Reihenfolge nach Fremdschlüsseln: Eltern vor Kindern.
TABLES = [
    ("customers", ["active", "focus"]),
    ("web_clients", ["active"]),
    ("app_users", ["active", "must_change_password"]),
    ("contracts", ["active"]),
    ("devices", ["online"]),
    ("sites", ["core_update", "backup_confirmed"]),
    ("runs", []),
    ("tasks", ["requires_approval", "approved"]),
    ("jobs", []),
    ("task_executions", []),
    ("time_entries", ["billable"]),
    ("audit_events", []),
    ("app_settings", []),
    ("sessions", []),
    ("device_patches", []),
    ("web_actions", []),
]


def quote(value):
    if value is None:
        return "NULL"
    if isinstance(value, bool):
        return "TRUE" if value else "FALSE"
    if isinstance(value, (int, float)):
        return str(value)
    text = str(value).replace("'", "''")
    return f"'{text}'"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--sqlite", default="data/cockpit.db", help="Pfad zur alten SQLite-Datei")
    parser.add_argument("--out", default="data/import.sql", help="Ziel für die SQL-Datei")
    args = parser.parse_args()

    source = Path(args.sqlite)
    if not source.exists():
        sys.exit(f"SQLite-Datei nicht gefunden: {source}")

    connection = sqlite3.connect(f"file:{source}?mode=ro", uri=True)
    connection.row_factory = sqlite3.Row

    lines = [
        "-- Automatisch erzeugt von scripts/import-sqlite-to-postgres.py",
        "-- Quelle: " + str(source),
        # Ohne diese Zeile rät psql die Kodierung aus der Windows-Codepage und
        # bricht bei Umlauten mit „ungültige Byte-Sequenz" ab.
        "SET client_encoding = 'UTF8';",
        "BEGIN;",
    ]

    total = 0
    for table, bool_columns in TABLES:
        try:
            rows = connection.execute(f"SELECT * FROM {table}").fetchall()
        except sqlite3.OperationalError:
            # Tabelle gibt es in der Altdatenbank nicht – überspringen.
            continue

        if not rows:
            print(f"  {table}: leer")
            continue

        columns = list(rows[0].keys())
        column_sql = ", ".join(columns)
        updates = ", ".join(
            f"{column} = EXCLUDED.{column}" for column in columns if column != "id"
        )

        for row in rows:
            values = []
            for column in columns:
                value = row[column]
                if column in bool_columns and value is not None:
                    value = bool(value)
                values.append(quote(value))

            lines.append(
                f"INSERT INTO {table} ({column_sql}) VALUES ({', '.join(values)}) "
                f"ON CONFLICT DO NOTHING;"
            )

        total += len(rows)
        print(f"  {table}: {len(rows)} Zeilen")

    lines.append("COMMIT;")

    target = Path(args.out)
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text("\n".join(lines) + "\n", encoding="utf-8")

    connection.close()
    print(f"\n{total} Zeilen geschrieben nach {target}")


if __name__ == "__main__":
    main()
