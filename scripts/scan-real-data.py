"""Sucht echte Kundendaten in den versionierten Dateien.

Die Suchbegriffe selbst sind vertraulich – deshalb stehen sie **nicht** im
Skript, sondern in `data/real-names.txt` (nicht versioniert, eine Zeile pro
Begriff). So wandert die Liste nie ins Repository, auch nicht in die Historie.

Einrichtung:
    data/real-names.txt anlegen, eine Zeile pro Begriff, z. B.
        Beispielkunde GmbH
        PC-EMPFAENG-01

Aufruf im Projektordner:
    python scripts/scan-real-data.py

Rückgabewert 1 bei Treffern – geeignet für einen Git-Hook.
"""

import subprocess
import sys
from pathlib import Path

PROJECT = Path(__file__).resolve().parent.parent
NAMES_FILE = PROJECT / "data" / "real-names.txt"

# Binärformate lassen sich nicht sinnvoll durchsuchen. Bilder müssen von Hand
# geprüft werden – dort steht Text in Pixeln.
SKIP_SUFFIX = {".png", ".jpg", ".jpeg", ".ico", ".icns", ".woff", ".woff2", ".ttf"}


def load_needles():
    if not NAMES_FILE.exists():
        return []

    return [
        line.strip()
        for line in NAMES_FILE.read_text(encoding="utf-8").splitlines()
        if line.strip() and not line.strip().startswith("#")
    ]


def tracked_files():
    out = subprocess.run(
        ["git", "ls-files"], capture_output=True, text=True, encoding="utf-8"
    )
    return [PROJECT / line for line in out.stdout.splitlines() if line.strip()]


def main():
    needles = load_needles()

    if not needles:
        print(f"Keine Suchbegriffe in {NAMES_FILE}")
        print("Datei anlegen (eine Zeile pro Begriff) und erneut starten.")
        return 0

    findings = []

    for path in tracked_files():
        if path.suffix.lower() in SKIP_SUFFIX or not path.is_file():
            continue

        try:
            text = path.read_text(encoding="utf-8", errors="ignore")
        except OSError:
            continue

        relative = path.relative_to(PROJECT)

        for number, line in enumerate(text.splitlines(), start=1):
            for needle in needles:
                if needle in line:
                    findings.append((str(relative), number, line.strip()[:110]))
                    break

    if not findings:
        print(f"Keine echten Daten gefunden ({len(needles)} Begriffe geprüft).")
        return 0

    print(f"{len(findings)} Treffer:\n")
    for path, number, line in findings:
        print(f"{path}:{number}")
        print(f"    {line}")

    return 1


if __name__ == "__main__":
    sys.exit(main())
