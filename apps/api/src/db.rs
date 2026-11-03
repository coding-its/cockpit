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

    sqlx::migrate!("./migrations_pg").run(&pool).await?;
    Ok(pool)
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

    pool.close().await;

    let short = version.split(" on ").next().unwrap_or(&version).to_string();
    Ok(format!("{short} · {tables} Tabellen im Schema public"))
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
