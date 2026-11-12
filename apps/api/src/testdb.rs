//! Gemeinsame Testdatenbank für die Datenbanktests.
//!
//! Die Tests laufen gegen eine eigene PostgreSQL-Datenbank, nicht gegen die
//! Produktivdaten. Adresse über `TEST_DATABASE_URL`, sonst `DATABASE_URL`,
//! sonst die Vorgabe unten.
//!
//! Alle Datenbanktests laufen nacheinander (gemeinsame Sperre) und leeren die
//! Tabellen zu Beginn. Parallel würden sie sich gegenseitig die Daten löschen.

use sqlx::PgPool;

pub fn url() -> String {
    std::env::var("TEST_DATABASE_URL")
        .or_else(|_| std::env::var("DATABASE_URL"))
        .unwrap_or_else(|_| {
            // Bewusst ohne Passwort: Testdatenbanken gehören nicht ins
            // Repository, und ein hinterlegtes Kennwort wäre hier für alle
            // lesbar. Ohne TEST_DATABASE_URL bricht der Test mit einem
            // Hinweis ab, statt an einer falschen Verbindung zu scheitern.
            "postgres://postgres@127.0.0.1:5432/wartungscockpit_test".to_string()
        })
}

static LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Verbindung zur Testdatenbank mit frischem Schema und leeren Tabellen.
pub async fn fixture() -> (tokio::sync::MutexGuard<'static, ()>, PgPool) {
    let guard = LOCK.lock().await;

    let pool = crate::db::init(&url())
        .await
        .unwrap_or_else(|error| {
            panic!(
                "Testdatenbank nicht erreichbar ({error}).\n\
                 TEST_DATABASE_URL setzen, z. B. \
                 postgres://postgres:passwort@127.0.0.1:5432/wartungscockpit_test"
            )
        });

    sqlx::query(
        "TRUNCATE activity_kinds, teamviewer_sessions, teamviewer_groups, accounts,
                  time_entries, device_patches, devices, sites, web_actions, web_clients,
                  tasks, jobs, task_executions, runs, contracts, customers,
                  sessions, app_users, audit_events, app_settings CASCADE",
    )
    .execute(&pool)
    .await
    .expect("Testdatenbank konnte nicht geleert werden");

    // Die Leistungsarten kommen aus einer Migration, die nur einmal läuft.
    // Nach dem Leeren müssen sie wieder da sein, sonst prüfen die Tests gegen
    // eine leere Liste. Bei Änderungen an 0006 hier nachziehen.
    sqlx::query(
        "INSERT INTO activity_kinds (id, name, position, active, created_at) VALUES
            ('ak-01', 'Remote Support', 10, true, '2026-11-12T00:00:00'),
            ('ak-02', 'Remote Support – außerhalb GZ', 20, true, '2026-11-12T00:00:00'),
            ('ak-03', 'Vor-Ort-Service', 30, true, '2026-11-12T00:00:00'),
            ('ak-04', 'Vor-Ort-Service – außerhalb GZ', 40, true, '2026-11-12T00:00:00'),
            ('ak-05', 'Systemadministration & Datenpflege', 50, true, '2026-11-12T00:00:00'),
            ('ak-06', 'System-Monitoring & Alert-Handling', 60, true, '2026-11-12T00:00:00'),
            ('ak-07', 'Backup-Überwachung & Validierung', 70, true, '2026-11-12T00:00:00'),
            ('ak-08', 'Endpoint-Hardware-Service', 80, true, '2026-11-12T00:00:00'),
            ('ak-09', 'Technische Analyse & Consulting', 90, true, '2026-11-12T00:00:00'),
            ('ak-10', 'Projektplanung & Konzeption', 100, true, '2026-11-12T00:00:00'),
            ('ak-11', 'Kundenkommunikation (E-Mail)', 110, true, '2026-11-12T00:00:00'),
            ('ak-12', 'Kundenkommunikation (Telefon)', 120, true, '2026-11-12T00:00:00'),
            ('ak-13', 'Buchhaltung', 130, true, '2026-11-12T00:00:00'),
            ('ak-14', 'Webdesign', 140, true, '2026-11-12T00:00:00')",
    )
    .execute(&pool)
    .await
    .expect("Leistungsarten konnten nicht angelegt werden");

    (guard, pool)
}

