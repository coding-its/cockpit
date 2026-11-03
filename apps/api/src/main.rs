//! Startprogramm für den Browserbetrieb.
//!
//! Die gesamte Logik liegt in der Bibliothek (`lib.rs`); hier wird sie nur
//! gestartet. Die Tauri-App benutzt dieselbe Bibliothek ohne HTTP-Server.

use std::sync::Arc;

use tracing_subscriber::EnvFilter;

use wartungscockpit_api::{build_app, start};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    // `.env` im Arbeitsverzeichnis, sonst neben der Datenbankdatei. Ohne den
    // zweiten Weg startet ein Aufruf aus einem anderen Ordner im Mock-Modus –
    // und würde NinjaOne-Aktionen nur simulieren statt auszuführen.
    let env_file = dotenvy::dotenv().ok().or_else(|| {
        let candidates = [
            wartungscockpit_api::DbConfig::path()
                .parent()
                .and_then(|data| data.parent())
                .map(|root| root.join(".env")),
            wartungscockpit_api::DbConfig::app_data_dir().map(|dir| dir.join(".env")),
        ];

        candidates
            .into_iter()
            .flatten()
            .find_map(|candidate| dotenvy::from_path(&candidate).ok().map(|_| candidate))
    });

    match env_file {
        Some(path) => tracing::info!(env_file = %path.display(), "Konfiguration geladen"),
        None => tracing::warn!("Keine .env gefunden – NinjaOne läuft im Mock-Modus"),
    }

    // Nothilfe ohne Datenbankwerkzeug:
    //   wartungscockpit-api --reset-admin-password [benutzername]
    let args: Vec<String> = std::env::args().collect();
    if let Some(index) = args.iter().position(|arg| arg == "--reset-admin-password") {
        let username = args
            .get(index + 1)
            .filter(|value| !value.starts_with("--"))
            .cloned()
            .unwrap_or_else(|| "admin".to_string());

        let password = wartungscockpit_api::reset_admin_password(&username).await?;

        println!();
        println!("  Benutzername: {username}");
        println!("  Neues Passwort: {password}");
        println!();
        println!("  Das Passwort ist ab jetzt gültig. Alle Sitzungen dieses Kontos wurden beendet.");
        println!();
        return Ok(());
    }

    let state = start().await?;
    let port = state.config.port;
    let static_dir = state.config.static_dir.clone();

    let app = build_app(state.clone(), Some(&static_dir));
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port)).await?;
    tracing::info!("API erreichbar unter http://127.0.0.1:{port}");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    let pool = Arc::clone(&state).pool.clone();
    pool.close().await;
    Ok(())
}

async fn shutdown_signal() {
    if tokio::signal::ctrl_c().await.is_ok() {
        tracing::info!("Shutdown angefordert");
    }
}
