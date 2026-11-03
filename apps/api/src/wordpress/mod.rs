//! Client für den WordPress-Maintenance-Agent (Mu-Plugin).
//!
//! Authentifizierung: HTTP Basic mit Application Password plus HMAC-SHA256
//! über `<timestamp>.<raw body>` mit dem site-spezifischen Secret.

use std::time::Duration;

use anyhow::{bail, Context, Result};
use base64::Engine;
use hmac::{Hmac, Mac};
use reqwest::Method;
use serde::{Deserialize, Serialize};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone)]
pub struct WpAgentConfig {
    pub site_url: String,
    pub user: String,
    pub app_password: String,
    pub secret: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WpHealth {
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub ok: Option<bool>,
    #[serde(default)]
    pub http_status: Option<i64>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub fatal_marker: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WpSiteInfo {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WpCoreInfo {
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub update_available: Option<bool>,
    #[serde(default)]
    pub new_version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WpPhpInfo {
    #[serde(default)]
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WpPluginStats {
    #[serde(default)]
    pub installed: Option<i64>,
    #[serde(default)]
    pub active: Option<i64>,
    #[serde(default)]
    pub updates_available: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WpHealthData {
    #[serde(default)]
    pub site: Option<WpSiteInfo>,
    #[serde(default)]
    pub core: Option<WpCoreInfo>,
    #[serde(default)]
    pub php: Option<WpPhpInfo>,
    #[serde(default)]
    pub plugins: Option<WpPluginStats>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WpPluginUpdateResult {
    #[serde(default)]
    pub file: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub version_before: Option<String>,
    #[serde(default)]
    pub version_after: Option<String>,
    #[serde(default)]
    pub success: Option<bool>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub skipped: Option<bool>,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WpUpdateResponse {
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub dry_run: Option<bool>,
    #[serde(default)]
    pub plugins: Option<Vec<WpPluginUpdateResult>>,
    #[serde(default)]
    pub health: Option<WpHealth>,
    // Core-Update-Felder
    #[serde(default)]
    pub version_before: Option<String>,
    #[serde(default)]
    pub version_after: Option<String>,
    #[serde(default)]
    pub current_version: Option<String>,
    #[serde(default)]
    pub target_version: Option<String>,
    #[serde(default)]
    pub success: Option<bool>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub output: Option<String>,
}

pub struct WpAgent {
    http: reqwest::Client,
    config: WpAgentConfig,
}

impl WpAgent {
    pub fn new(config: WpAgentConfig) -> Result<Self> {
        if config.app_password.trim().is_empty() {
            bail!("Kein Application Password für diese Site hinterlegt");
        }
        if config.secret.trim().is_empty() {
            bail!("Kein Agent-Secret für diese Site hinterlegt");
        }

        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(180))
            .user_agent("Wartungscockpit/0.1")
            .build()?;

        Ok(Self { http, config })
    }

    pub async fn health(&self) -> Result<WpHealthData> {
        #[derive(Deserialize)]
        struct Envelope {
            #[serde(default)]
            health: Option<WpHealthData>,
        }

        let envelope: Envelope = self.request(Method::GET, "/health", "").await?;
        envelope
            .health
            .ok_or_else(|| anyhow::anyhow!("Antwort enthielt keinen health-Block"))
    }

    #[allow(dead_code)]
    pub async fn inventory(&self) -> Result<(WpHealthData, serde_json::Value)> {
        #[derive(Deserialize)]
        struct Envelope {
            #[serde(default)]
            health: Option<WpHealthData>,
            #[serde(default)]
            plugins: serde_json::Value,
        }

        let envelope: Envelope = self
            .request(Method::GET, "/inventory?refresh=true", "")
            .await?;

        Ok((
            envelope.health.unwrap_or(WpHealthData {
                site: None,
                core: None,
                php: None,
                plugins: None,
            }),
            envelope.plugins,
        ))
    }

    /// `dry_run = true` zeigt nur die geplanten Änderungen.
    pub async fn update_plugins(
        &self,
        dry_run: bool,
        backup_confirmed: bool,
    ) -> Result<WpUpdateResponse> {
        let body = serde_json::json!({
            "dry_run": dry_run,
            "backup_confirmed": backup_confirmed,
            "continue_on_error": false,
        })
        .to_string();

        self.request(Method::POST, "/updates/plugins", &body).await
    }

    pub async fn update_core(
        &self,
        dry_run: bool,
        backup_confirmed: bool,
    ) -> Result<WpUpdateResponse> {
        let body = serde_json::json!({
            "dry_run": dry_run,
            "backup_confirmed": backup_confirmed,
        })
        .to_string();

        self.request(Method::POST, "/updates/core", &body).await
    }

    async fn request<T: for<'de> Deserialize<'de>>(
        &self,
        method: Method,
        path: &str,
        body: &str,
    ) -> Result<T> {
        let base = self.config.site_url.trim_end_matches('/');
        let url = format!("{base}/wp-json/maintenance/v1{path}");
        let timestamp = chrono::Utc::now().timestamp().to_string();

        let mut mac = HmacSha256::new_from_slice(self.config.secret.as_bytes())
            .context("HMAC-Schlüssel ungültig")?;
        mac.update(timestamp.as_bytes());
        mac.update(b".");
        mac.update(body.as_bytes());
        let signature = hex(&mac.finalize().into_bytes());

        let basic = base64::engine::general_purpose::STANDARD.encode(format!(
            "{}:{}",
            self.config.user, self.config.app_password
        ));

        let mut request = self
            .http
            .request(method, &url)
            .header("Authorization", format!("Basic {basic}"))
            .header("X-Maintenance-Timestamp", &timestamp)
            .header("X-Maintenance-Signature", &signature);

        if !body.is_empty() {
            request = request
                .header("Content-Type", "application/json")
                .body(body.to_string());
        }

        let response = request.send().await.with_context(|| {
            format!("WordPress-Agent nicht erreichbar: {url}")
        })?;

        let status = response.status();
        let text = response.text().await.unwrap_or_default();

        if !status.is_success() {
            let hint = match status.as_u16() {
                401 => " – Signatur oder Application Password falsch",
                403 => " – fehlende Berechtigung (update_plugins/update_core)",
                409 => " – Aktion gesperrt (Backup-Bestätigung fehlt)",
                404 => " – Agent nicht installiert oder Rewrite-Regeln fehlen",
                _ => "",
            };
            bail!(
                "WordPress-Agent antwortete mit {status}{hint}: {}",
                truncate(&text, 300)
            );
        }

        serde_json::from_str(&text)
            .with_context(|| format!("Antwort nicht lesbar: {}", truncate(&text, 200)))
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn truncate(value: &str, max: usize) -> String {
    let mut chars = value.chars();
    let truncated: String = chars.by_ref().take(max).collect();
    if chars.next().is_some() {
        format!("{truncated}…")
    } else {
        truncated
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> WpAgentConfig {
        WpAgentConfig {
            site_url: "http://localhost:81/wordpress".to_string(),
            user: "wartung-cockpit".to_string(),
            app_password: "abcd efgh".to_string(),
            secret: "geheim".to_string(),
        }
    }

    #[test]
    fn rejects_missing_password() {
        let mut cfg = config();
        cfg.app_password = "  ".to_string();
        assert!(WpAgent::new(cfg).is_err());
    }

    #[test]
    fn rejects_missing_secret() {
        let mut cfg = config();
        cfg.secret = String::new();
        assert!(WpAgent::new(cfg).is_err());
    }

    #[test]
    fn accepts_complete_config() {
        assert!(WpAgent::new(config()).is_ok());
    }
}
