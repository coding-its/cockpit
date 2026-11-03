# ADR 0001 – Modularer Monolith mit Rust/Axum und Vue 3

**Status:** entschieden · Oktober 2026

## Kontext

Das Cockpit wird von einem kleinen Systemhaus selbst betrieben. Es braucht eine Weboberfläche für mehrere Techniker, eine verlässliche Ausführungsschicht für externe APIs und eine einfache Deployment-Geschichte. Ein Microservice-Split wäre für dieses Team und diesen Umfang zusätzlicher Betriebsaufwand ohne Nutzen.

## Entscheidung

Ein modularer Monolith:

- Vue 3 + TypeScript als SPA, ausgeliefert vom Backend oder über Vite im Dev-Modus.
- Rust mit Axum und SQLx als API, Worker und statischer Server.
- Klare Module für Routen, Ausführung, NinjaOne-Client, Abrechnung und Datenzugriff.
- PostgreSQL ab Phase 1; SQLite im PoC.
- Kein Tauri im ersten Schritt. Die SPA kann später unverändert in eine Tauri-Hülle wandern.

## Konsequenzen

**Positiv:** ein Deployment, ein Datenmodell, keine Netzwerkkomplexität zwischen Services, Typsicherheit im Backend, gute Performance und ein schlankes Binary.

**Negativ:** Rust erhöht die Einstiegshürde für PHP-/JavaScript-affine Teams. Der Monolith muss diszipliniert modular bleiben, sonst entsteht ein unwartbarer Block. Hintergrundjobs und HTTP teilen sich den Prozess; bei Last später Worker separat starten.

**Alternativen:** Node/NestJS wäre teamnäher, aber weniger typsicher und schwerer zu deployen. Go wäre eine gute Alternative, Rust wurde wegen der bestehenden Präferenz und der starken Typisierung gewählt. Tauri als reine Desktop-App scheidet aus, weil mehrere Techniker gemeinsam auf Kunden- und Zeitdaten zugreifen müssen.
