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
        "TRUNCATE teamviewer_sessions, teamviewer_groups, accounts, time_entries, device_patches, devices, sites, web_actions, web_clients,
                  tasks, jobs, task_executions, runs, contracts, customers,
                  sessions, app_users, audit_events, app_settings CASCADE",
    )
    .execute(&pool)
    .await
    .expect("Testdatenbank konnte nicht geleert werden");

    (guard, pool)
}
