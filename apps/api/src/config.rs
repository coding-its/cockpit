use std::env;

#[derive(Clone, Debug)]
pub struct NinjaConfig {
    pub base_url: String,
    pub client_id: String,
    pub client_secret: String,
    pub scope: String,
    pub mock: bool,
    /// Zweite App mit Benutzerkontext (Authorization Code + Refresh Token).
    /// Nötig für script/run: NinjaOne verlangt dort einen echten Benutzer.
    pub user_client_id: String,
    pub user_client_secret: String,
    pub redirect_uri: String,
}

#[derive(Clone, Debug)]
pub struct Config {
    pub port: u16,
    pub db: crate::dbconfig::DbConfig,
    pub static_dir: String,
    pub ninjaone: NinjaConfig,
    pub wp_shared_secret: String,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let base_url = env::var("NINJAONE_BASE_URL")
            .unwrap_or_else(|_| "https://eu.ninjarmm.com".to_string());
        let client_id = env::var("NINJAONE_CLIENT_ID").unwrap_or_default();
        let client_secret = env::var("NINJAONE_CLIENT_SECRET").unwrap_or_default();
        let scope = env::var("NINJAONE_SCOPE")
            .unwrap_or_else(|_| "monitoring management".to_string());
        let mock = match env::var("NINJAONE_MOCK") {
            Ok(value) => !matches!(value.to_ascii_lowercase().as_str(), "0" | "false" | "no"),
            Err(_) => client_id.is_empty() || client_secret.is_empty(),
        };

        let port = env::var("PORT")
            .unwrap_or_else(|_| "8080".to_string())
            .parse::<u16>()
            .map_err(|_| anyhow::anyhow!("PORT muss eine Zahl zwischen 1 und 65535 sein"))?;

        let (db, source) = crate::dbconfig::DbConfig::load();
        if let Some(source) = source {
            tracing::info!(source = %source, ziel = %db.describe(), "Datenbankverbindung geladen");
        } else {
            tracing::warn!(
                ziel = %db.describe(),
                "Keine Datenbankverbindung hinterlegt – Vorgaben werden benutzt"
            );
        }

        Ok(Self {
            port,
            db,
            static_dir: env::var("STATIC_DIR")
                .unwrap_or_else(|_| "apps/web/dist".to_string()),
            ninjaone: NinjaConfig {
                base_url,
                client_id,
                client_secret,
                scope,
                mock,
                user_client_id: env::var("NINJAONE_USER_CLIENT_ID").unwrap_or_default(),
                user_client_secret: env::var("NINJAONE_USER_CLIENT_SECRET").unwrap_or_default(),
                redirect_uri: env::var("NINJAONE_REDIRECT_URI")
                    .unwrap_or_else(|_| "http://127.0.0.1:8080/api/v1/auth/ninjaone/callback".to_string()),
            },
            wp_shared_secret: env::var("WP_AGENT_SHARED_SECRET")
                .unwrap_or_else(|_| "change-me-in-production".to_string()),
        })
    }
}
