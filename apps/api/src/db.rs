use std::str::FromStr;
use std::time::Duration;

use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::PgPool;

/// Baut den Verbindungspool zur PostgreSQL-Datenbank und spielt das Schema ein.
///
/// Früher war das eine lokale SQLite-Datei. Der Wechsel war nötig, weil mehrere
/// Personen gleichzeitig auf dieselben Daten zugreifen – das darf keine Datei
/// über eine Netzwerkfreigabe lösen.
pub async fn init(url: &str) -> anyhow::Result<PgPool> {
    let options = PgConnectOptions::from_str(url)
        .map_err(|error| anyhow::anyhow!("Datenbankadresse unbrauchbar: {error}"))?
        .application_name("wartungscockpit");

    let pool = PgPoolOptions::new()
        .max_connections(10)
        // Ohne Zeitlimit hängt der Start minutenlang, wenn der Server nicht
        // antwortet – der Benutzer sieht nur ein leeres Fenster.
        .acquire_timeout(Duration::from_secs(8))
        .connect_with(options)
        .await
        .map_err(|error| {
            anyhow::anyhow!(
                "Keine Verbindung zur Datenbank. Bitte Adresse, Port, Datenbankname und \
                 Zugangsdaten prüfen (Einstellungen → Datenbank). Meldung: {}",
                friendly(&error.to_string())
            )
        })?;

    sqlx::migrate!("./migrations_pg")
        .run(&pool)
        .await
        .map_err(|error| anyhow::anyhow!("{}", migration_message(&error)))?;
    Ok(pool)
}

/// Übersetzt Schemafehler in etwas, das man lesen kann.
///
/// Der wichtigste Fall: Die Datenbank wurde von einer neueren Version des
/// Cockpits eingerichtet. sqlx bricht dann ab, weil es das Schema nicht
/// kennt – das ist richtig so, aber die Meldung („migration 2 was previously
/// applied but is missing …") klingt für den Benutzer nach kaputter Datenbank.
/// Es fehlt nur die neue Version der Anwendung.
fn migration_message(error: &sqlx::migrate::MigrateError) -> String {
    use sqlx::migrate::MigrateError;

    match error {
        MigrateError::VersionMissing(version) => format!(
            "Die Datenbank ist erreichbar. Sie wurde aber mit einer neueren Version des \
             Wartungscockpits eingerichtet (Schema-Stand {version}). Diese installierte \
             Version kennt diesen Stand nicht.\n\n\
             Bitte das Wartungscockpit aktualisieren. An den Daten ist nichts beschädigt."
        ),
        MigrateError::VersionMismatch(version) => format!(
            "Schema-Stand {version} passt nicht zu dieser Version des Wartungscockpits. \
             Vermutlich wurde auf dem Server eine ältere Version eingespielt.\n\n\
             Bitte die aktuelle Version installieren."
        ),
        other => format!(
            "Die Datenbank ist erreichbar, das Schema ließ sich aber nicht prüfen: {other}"
        ),
    }
}

/// Höchster Schema-Stand, den diese Programmversion kennt.
fn known_schema_version() -> i64 {
    sqlx::migrate!("./migrations_pg")
        .migrations
        .iter()
        .map(|migration| migration.version)
        .max()
        .unwrap_or(0)
}

/// Prüft eine Verbindung, ohne den laufenden Betrieb anzufassen.
/// Für den Knopf „Verbindung testen" in den Einstellungen.
pub async fn test(url: &str) -> Result<String, String> {
    let options = PgConnectOptions::from_str(url).map_err(|error| error.to_string())?;

    let pool = PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(Duration::from_secs(6))
        .connect_with(options)
        .await
        .map_err(|error| friendly(&error.to_string()))?;

    let version: String = sqlx::query_scalar("SELECT version()")
        .fetch_one(&pool)
        .await
        .map_err(|error| friendly(&error.to_string()))?;

    let tables: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM information_schema.tables WHERE table_schema = 'public'",
    )
    .fetch_one(&pool)
    .await
    .unwrap_or(0);

    // Der Schema-Stand gehört zum Test: Sonst meldet der Knopf „passt", und
    // der Start scheitert danach daran, dass die Datenbank von einer neueren
    // Version eingerichtet wurde.
    let applied: Option<i64> =
        sqlx::query_scalar("SELECT MAX(version) FROM _sqlx_migrations WHERE success")
            .fetch_one(&pool)
            .await
            .ok()
            .flatten();

    pool.close().await;

    let known = known_schema_version();
    if let Some(applied) = applied {
        if applied > known {
            return Err(format!(
                "Die Datenbank ist erreichbar, wurde aber von einer neueren Version des \
                 Wartungscockpits eingerichtet (Schema-Stand {applied}, diese Version kennt \
                 {known}).\n\nBitte das Wartungscockpit aktualisieren – an den Daten ist \
                 nichts beschädigt."
            ));
        }
    }

    let short = version.split(" on ").next().unwrap_or(&version).to_string();
    let schema = match applied {
        Some(applied) if applied < known => {
            format!("Schema-Stand {applied}, wird beim Start auf {known} ergänzt")
        }
        Some(applied) => format!("Schema-Stand {applied}"),
        None => "noch kein Schema – wird beim Start angelegt".to_string(),
    };

    Ok(format!("{short} · {tables} Tabellen im Schema public · {schema}"))
}

/// Übersetzt die häufigsten Verbindungsfehler in etwas, das man lesen kann.
/// Die Meldungen des Treibers sind englisch und sprechen von Pools und
/// Protokollen, nicht von Servern und Passwörtern.
fn friendly(message: &str) -> String {
    let lower = message.to_lowercase();

    // Beim Anmeldefehler antwortet Windows-PostgreSQL in der Systemcodepage,
    // bevor die Zeichenkodierung ausgehandelt ist. Der Treiber kann die
    // Meldung dann nicht lesen – wir übersetzen sie ins Verständliche.
    if lower.contains("non-utf-8") {
        return "Der Server hat die Verbindung abgelehnt. Häufigste Ursachen: Benutzername \
                oder Passwort stimmt nicht, oder die Datenbank existiert noch nicht."
            .to_string();
    }
    if lower.contains("timed out") || lower.contains("connection refused") {
        return "Server nicht erreichbar. Läuft PostgreSQL, stimmt die Adresse und ist der Port in der Firewall freigegeben?"
            .to_string();
    }
    if lower.contains("password") || lower.contains("authentification") {
        return "Benutzername oder Passwort stimmt nicht.".to_string();
    }
    if lower.contains("does not exist") && lower.contains("database") {
        return "Diese Datenbank gibt es auf dem Server nicht. Bitte zuerst anlegen.".to_string();
    }
    if lower.contains("no pg_hba.conf entry") {
        return "Der Server erlaubt diesen Rechner nicht (pg_hba.conf). In PostgreSQL muss die Verbindung von dieser Adresse zugelassen sein."
            .to_string();
    }

    message.to_string()
}

#[cfg(test)]
mod tests {
    use super::{friendly, known_schema_version, migration_message};
    use sqlx::migrate::MigrateError;

    #[test]
    fn a_too_new_schema_points_to_the_application_version() {
        let message = migration_message(&MigrateError::VersionMissing(2));

        // Der Benutzer soll nicht nach Passwort oder Server suchen.
        assert!(message.contains("Datenbank ist erreichbar"), "{message}");
        assert!(message.contains("aktualisieren"), "{message}");
        assert!(message.contains("nichts beschädigt"), "{message}");
        assert!(!message.contains("Passwort"), "{message}");
    }

    #[test]
    fn known_schema_version_matches_the_shipped_migrations() {
        // 0001_schema, 0002_billed, 0003_accounts, 0004_teamviewer –
        // wächst mit jeder neuen Datei.
        assert_eq!(known_schema_version(), 4);
    }

    #[test]
    fn connection_errors_stay_readable() {
        assert!(friendly("connection refused").contains("Port"));
        assert!(friendly("no pg_hba.conf entry for host").contains("pg_hba.conf"));
        assert!(friendly("Etwas ganz anderes") == "Etwas ganz anderes");
    }
}
