//! Das Wartungscockpit als Bibliothek.
//!
//! Das Startprogramm (`main.rs`) startet daraus den HTTP-Server für den
//! Browserbetrieb. Die Tauri-App bindet dieselbe Bibliothek ein und ruft den
//! Router **im eigenen Prozess** auf – ohne Netzwerk, ohne offenen Port.
//! Beide Wege benutzen also dieselben Routen, dieselbe Rechteprüfung und
//! denselben Audit-Trail.

mod auth;
mod billing;
mod config;
mod db;
mod dbconfig;
mod envfile;
mod error;
mod executors;
mod export;
mod models;
mod monitoring;
mod ninjaone;
mod queries;
mod reports;
mod routes;
mod seed;
mod settings;
mod statement;
mod state;
mod util;
mod wordpress;

#[cfg(test)]
mod testdb;

use axum::http::header;
use axum::Router;
use tower_http::cors::CorsLayer;
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;

pub use crate::dbconfig::DbConfig;
pub use crate::state::SharedState;

/// Baut Datenbankverbindung, Erstbefüllung und NinjaOne-Client auf.
///
/// Beide Betriebsarten (Browser und Desktop) brauchen genau das, deshalb
/// steht es hier und nicht im Startprogramm.
pub async fn start() -> anyhow::Result<SharedState> {
    let config = config::Config::from_env()?;

    let pool = db::init(&config.db.url()).await?;
    seed::ensure(&pool).await?;
    auth::ensure_admin(&pool).await?;

    if let Ok(removed) = auth::purge_expired(&pool).await {
        if removed > 0 {
            tracing::info!(count = removed, "Abgelaufene Sitzungen entfernt");
        }
    }

    let ninja = ninjaone::NinjaOneClient::new(config.ninjaone.clone());

    if let Ok(Some(token)) = sqlx::query_scalar::<_, String>(
        "SELECT value FROM app_settings WHERE key = 'ninjaone.user_refresh_token'",
    )
    .fetch_optional(&pool)
    .await
    {
        ninja.set_user_refresh_token(Some(token)).await;
        tracing::info!("NinjaOne-Benutzerkontext aus Datenbank geladen");
    }

    tracing::info!(
        ninjaone_mode = if ninja.is_mock() { "mock" } else { "live" },
        database = %config.db.describe(),
        user_app_configured = ninja.user_app_configured(),
        "Wartungscockpit bereit"
    );

    Ok(std::sync::Arc::new(state::AppState {
        pool,
        ninja,
        config,
    }))
}

/// Alle Routen mit Sitzungsprüfung. `static_dir` liefert die Oberfläche aus;
/// ohne Angabe antwortet der Router nur unter `/api/v1` (Desktop-Betrieb).
pub fn build_app(state: SharedState, static_dir: Option<&str>) -> Router {
    let app = routes::router(state.clone()).layer(axum::middleware::from_fn_with_state(
        state.clone(),
        auth::guard,
    ));

    let app = match static_dir {
        Some(dir) => app.fallback_service({
            let index = format!("{dir}/index.html");
            ServeDir::new(dir.to_string()).fallback(ServeFile::new(index))
        }),
        None => app,
    };

    app
        // Ohne Cache-Angabe halten Browser alte Bundles beliebig lange im
        // Speicher – nach einem Update sieht man dann die alte Oberfläche.
        // `no-cache` erlaubt Zwischenspeichern, erzwingt aber Rückfrage; die
        // Dateinamen enthalten einen Hash, sind also ohnehin eindeutig.
        .layer(SetResponseHeaderLayer::overriding(
            header::CACHE_CONTROL,
            axum::http::HeaderValue::from_static("no-cache"),
        ))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
}

/// Prüft eine Datenbankverbindung, ohne den laufenden Betrieb anzufassen.
/// Wird vom Einrichtungsfenster gebraucht, bevor eine Verbindung gespeichert wird.
pub async fn test_connection(url: &str) -> Result<String, String> {
    db::test(url).await
}

/// Setzt ein neues Passwort für ein Konto und beendet dessen Sitzungen.
/// Für `--reset-admin-password` auf der Kommandozeile.
pub async fn reset_admin_password(username: &str) -> anyhow::Result<String> {
    let config = config::Config::from_env()?;
    let pool = db::init(&config.db.url()).await?;

    let exists: Option<String> =
        sqlx::query_scalar("SELECT id FROM app_users WHERE username = $1")
            .bind(username)
            .fetch_optional(&pool)
            .await?;

    let Some(user_id) = exists else {
        pool.close().await;
        anyhow::bail!("Benutzer {username} nicht gefunden");
    };

    let password = random_password();
    let hash = auth::hash_password(&password)
        .map_err(|error| anyhow::anyhow!("{error}"))?;

    sqlx::query(
        "UPDATE app_users SET password_hash = $1, must_change_password = false, active = true WHERE id = $2",
    )
    .bind(&hash)
    .bind(&user_id)
    .execute(&pool)
    .await?;

    sqlx::query("DELETE FROM sessions WHERE user_id = $1")
        .bind(&user_id)
        .execute(&pool)
        .await?;

    auth::audit_password_reset(&pool, username).await.ok();
    pool.close().await;

    Ok(password)
}

fn random_password() -> String {
    use base64::Engine;
    use rand::RngCore;

    let mut bytes = [0u8; 9];
    rand::thread_rng().fill_bytes(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}
