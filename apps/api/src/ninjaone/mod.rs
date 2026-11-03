use std::time::Duration;

use anyhow::{bail, Context, Result};
use reqwest::{Method, StatusCode};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::config::NinjaConfig;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Organization {
    pub id: i64,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub node_approval_mode: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NinjaDevice {
    pub id: i64,
    #[serde(default)]
    pub system_name: Option<String>,
    #[serde(default)]
    pub dns_name: Option<String>,
    #[serde(default)]
    pub node_class: Option<String>,
    #[serde(default)]
    pub offline: Option<bool>,
    #[serde(default)]
    pub last_contact: Option<f64>,
    #[serde(default)]
    pub os: Option<serde_json::Value>,
    #[serde(default)]
    pub device_type: Option<String>,
    #[serde(default)]
    pub online: Option<bool>,
}

/// Rohe Antwort einer schreibenden NinjaOne-Aktion.
#[derive(Debug, Clone, Serialize)]
pub struct UpstreamOutcome {
    pub status: u16,
    pub body: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceActivity {
    pub id: i64,
    #[serde(default)]
    pub activity_type: Option<String>,
    #[serde(default)]
    pub status_code: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub activity_time: Option<f64>,
}

/// Ein Patch-Eintrag aus `/v2/queries/os-patches`.
///
/// `status` ist `APPROVED`, `REJECTED`, `DELAYED` oder `MANUAL` – abgelehnt
/// heißt: jemand hat das Update bewusst nicht freigegeben.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OsPatch {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub severity: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub patch_type: Option<String>,
    #[serde(default)]
    pub kb_number: Option<String>,
    pub device_id: i64,
    #[serde(default)]
    pub timestamp: Option<f64>,
}

/// Ein Software-Patch aus `/v2/queries/software-patches`.
///
/// Anderer Aufbau als bei den Betriebssystem-Patches: kein `name`, sondern
/// `title`, kein `severity`, sondern `impact`.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SoftwarePatch {
    pub id: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub impact: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub patch_type: Option<String>,
    #[serde(default)]
    pub product_identifier: Option<String>,
    pub device_id: i64,
    #[serde(default)]
    pub timestamp: Option<f64>,
}

/// Ein Antivirus-Produkt auf einem Gerät aus `/v2/queries/antivirus-status`.
///
/// Ein Gerät meldet mehrere Produkte – typisch Bitdefender aktiv und Defender
/// abgeschaltet. `product_state` ist `ON`, `OFF` oder `Expired`.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AntivirusEntry {
    #[serde(default)]
    pub product_name: Option<String>,
    #[serde(default)]
    pub product_state: Option<String>,
    #[serde(default)]
    pub definition_status: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    pub device_id: i64,
    #[serde(default)]
    pub timestamp: Option<f64>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct AuthorizedTokens {
    pub refresh_token: String,
    pub access_token: String,
    pub expires_in: i64,
    /// Feldnamen der Token-Antwort, für die Diagnose im Callback.
    pub field_names: Vec<String>,
}

#[derive(Debug, Clone)]
struct CachedToken {
    value: String,
    expires_at: i64,
}

pub struct NinjaOneClient {
    http: reqwest::Client,
    config: NinjaConfig,
    token: RwLock<Option<CachedToken>>,
    user_token: RwLock<Option<CachedToken>>,
    user_refresh: RwLock<Option<String>>,
}

impl NinjaOneClient {
    pub fn new(config: NinjaConfig) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .user_agent("Wartungscockpit/0.1")
            .build()
            .expect("HTTP-Client konnte nicht erstellt werden");

        Self {
            http,
            config,
            token: RwLock::new(None),
            user_token: RwLock::new(None),
            user_refresh: RwLock::new(None),
        }
    }

    pub fn is_mock(&self) -> bool {
        self.config.mock
    }

    pub fn base_url(&self) -> &str {
        &self.config.base_url
    }

    /// Scope für den Benutzer-Flow. `offline_access` ist Pflicht, sonst
    /// liefert NinjaOne im Authorization-Code-Flow keinen Refresh-Token.
    fn user_scope(&self) -> String {
        if self.config.scope.split_whitespace().any(|part| part == "offline_access") {
            self.config.scope.clone()
        } else {
            format!("{} offline_access", self.config.scope.trim())
        }
    }

    pub fn user_app_configured(&self) -> bool {
        !self.config.user_client_id.is_empty() && !self.config.user_client_secret.is_empty()
    }

    pub async fn has_user_context(&self) -> bool {
        self.user_refresh.read().await.is_some()
    }

    pub async fn set_user_refresh_token(&self, refresh_token: Option<String>) {
        *self.user_refresh.write().await = refresh_token;
        *self.user_token.write().await = None;
    }

    #[allow(dead_code)]
    pub async fn user_refresh_token(&self) -> Option<String> {
        self.user_refresh.read().await.clone()
    }

    /// Zustimmungs-URL der Benutzer-App (Authorization Code).
    pub fn authorize_url(&self, state: &str) -> Option<String> {
        if !self.user_app_configured() {
            return None;
        }

        Some(format!(
            "{}/oauth/authorize?response_type=code&client_id={}&redirect_uri={}&scope={}&state={}",
            self.config.base_url.trim_end_matches('/'),
            urlencode(&self.config.user_client_id),
            urlencode(&self.config.redirect_uri),
            urlencode(&self.user_scope()),
            urlencode(state),
        ))
    }

    /// Tauscht den Authorization Code in Access- und Refresh-Token.
    pub async fn exchange_code(&self, code: &str) -> Result<AuthorizedTokens> {
        if !self.user_app_configured() {
            bail!("Keine Benutzer-App konfiguriert (NINJAONE_USER_CLIENT_ID/SECRET fehlen)");
        }

        let url = format!("{}/oauth/token", self.config.base_url.trim_end_matches('/'));
        let response = self
            .http
            .post(&url)
            .form(&[
                ("grant_type", "authorization_code"),
                ("code", code),
                ("client_id", self.config.user_client_id.as_str()),
                ("client_secret", self.config.user_client_secret.as_str()),
                ("redirect_uri", self.config.redirect_uri.as_str()),
            ])
            .send()
            .await
            .context("NinjaOne Code-Tausch fehlgeschlagen")?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            bail!(
                "NinjaOne Code-Tausch fehlgeschlagen ({status}): {}",
                truncate(&body, 300)
            );
        }

        let body = response
            .text()
            .await
            .context("NinjaOne Token-Antwort konnte nicht gelesen werden")?;

        let parsed: serde_json::Value = serde_json::from_str(&body)
            .context("NinjaOne Token-Antwort ist kein JSON")?;

        let field_names: Vec<String> = parsed
            .as_object()
            .map(|map| map.keys().cloned().collect())
            .unwrap_or_default();

        let access_token = parsed
            .get("access_token")
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .to_string();

        if access_token.is_empty() {
            bail!(
                "NinjaOne lieferte kein access_token. Antwortfelder: {}",
                field_names.join(", ")
            );
        }

        let refresh_token = parsed
            .get("refresh_token")
            .and_then(|value| value.as_str())
            .map(str::to_string);

        if refresh_token.is_none() {
            tracing::warn!(
                fields = %field_names.join(", "),
                "NinjaOne lieferte keinen refresh_token – vermutlich ist der Grant-Typ 'Aktualisierungstoken' in der App nicht erlaubt"
            );
        }

        let expires_in = parsed
            .get("expires_in")
            .and_then(|value| value.as_i64())
            .unwrap_or(3600);

        Ok(AuthorizedTokens {
            refresh_token: refresh_token.unwrap_or_default(),
            access_token,
            expires_in,
            field_names,
        })
    }

    /// Nutzt den gecachten Benutzer-Token oder erneuert ihn per Refresh-Token.
    pub async fn token_user(&self) -> Result<String> {
        {
            let cached = self.user_token.read().await;
            if let Some(token) = cached.as_ref() {
                if token.expires_at > chrono::Utc::now().timestamp() + 60 {
                    return Ok(token.value.clone());
                }
            }
        }

        let refresh = self
            .user_refresh
            .read()
            .await
            .clone()
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Kein NinjaOne-Benutzer verbunden. Unter Integrationen verbinden – script/run verlangt Benutzerkontext."
                )
            })?;

        let mut guard = self.user_token.write().await;
        if let Some(token) = guard.as_ref() {
            if token.expires_at > chrono::Utc::now().timestamp() + 60 {
                return Ok(token.value.clone());
            }
        }

        let url = format!("{}/oauth/token", self.config.base_url.trim_end_matches('/'));
        let response = self
            .http
            .post(&url)
            .form(&[
                ("grant_type", "refresh_token"),
                ("refresh_token", refresh.as_str()),
                ("client_id", self.config.user_client_id.as_str()),
                ("client_secret", self.config.user_client_secret.as_str()),
                ("scope", self.user_scope().as_str()),
            ])
            .send()
            .await
            .context("NinjaOne Refresh-Anfrage fehlgeschlagen")?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            bail!(
                "NinjaOne Refresh-Token ungültig ({status}): {}. Bitte Benutzer unter Integrationen neu verbinden.",
                truncate(&body, 200)
            );
        }

        #[derive(Deserialize)]
        struct TokenResponse {
            access_token: String,
            refresh_token: Option<String>,
            expires_in: Option<i64>,
        }

        let token: TokenResponse = response
            .json()
            .await
            .context("NinjaOne Refresh-Antwort konnte nicht gelesen werden")?;

        if let Some(new_refresh) = token.refresh_token.clone() {
            *self.user_refresh.write().await = Some(new_refresh);
        }

        let expires_at = chrono::Utc::now().timestamp() + token.expires_in.unwrap_or(3600);
        *guard = Some(CachedToken {
            value: token.access_token.clone(),
            expires_at,
        });

        Ok(token.access_token)
    }

    /// Aktivitäten eines Geräts: der eigentliche Nachweis für Patch- und Script-Läufe.
    pub async fn list_device_activities(&self, device_id: i64) -> Result<Vec<DeviceActivity>> {
        if self.config.mock {
            return Ok(mock_activities(device_id));
        }

        let response = self
            .send(
                Method::GET,
                &format!("/v2/device/{device_id}/activities"),
                None,
            )
            .await?;
        let response = self.ensure_success(response).await?;

        #[derive(Deserialize)]
        struct ActivityResponse {
            #[serde(default)]
            activities: Vec<DeviceActivity>,
        }

        let parsed: ActivityResponse = response
            .json()
            .await
            .context("NinjaOne-Aktivitäten konnten nicht gelesen werden")?;
        Ok(parsed.activities)
    }

    pub async fn list_organizations(&self) -> Result<Vec<Organization>> {
        if self.config.mock {
            return Ok(mock_organizations());
        }

        let response = self
            .send(Method::GET, "/v2/organizations", None)
            .await?;
        let response = self.ensure_success(response).await?;
        response
            .json()
            .await
            .context("NinjaOne-Organisationen konnten nicht gelesen werden")
    }

    pub async fn list_devices(&self, organization_id: i64) -> Result<Vec<NinjaDevice>> {
        if self.config.mock {
            return Ok(mock_devices(organization_id));
        }

        let response = self
            .send(
                Method::GET,
                &format!("/v2/organization/{organization_id}/devices"),
                None,
            )
            .await?;
        let response = self.ensure_success(response).await?;
        response
            .json()
            .await
            .context("NinjaOne-Geräte konnten nicht gelesen werden")
    }

    /// Alle offenen und abgelehnten Betriebssystem-Patches im Bestand.
    ///
    /// Ein Aufruf für den ganzen Mandanten statt einer Abfrage pro Gerät.
    pub async fn os_patches(&self) -> Result<Vec<OsPatch>> {
        if self.config.mock {
            return Ok(mock_os_patches());
        }

        #[derive(Deserialize)]
        struct QueryResponse {
            #[serde(default)]
            results: Vec<OsPatch>,
        }

        let response = self
            .send(Method::GET, "/v2/queries/os-patches", None)
            .await?;
        let response = self.ensure_success(response).await?;
        let parsed: QueryResponse = response
            .json()
            .await
            .context("NinjaOne-Patchdaten konnten nicht gelesen werden")?;
        Ok(parsed.results)
    }

    /// Software-Patches (Drittanbieter-Programme) im ganzen Bestand.
    pub async fn software_patches(&self) -> Result<Vec<SoftwarePatch>> {
        if self.config.mock {
            return Ok(mock_software_patches());
        }

        #[derive(Deserialize)]
        struct QueryResponse {
            #[serde(default)]
            results: Vec<SoftwarePatch>,
        }

        let response = self
            .send(Method::GET, "/v2/queries/software-patches", None)
            .await?;
        let response = self.ensure_success(response).await?;
        let parsed: QueryResponse = response
            .json()
            .await
            .context("NinjaOne-Software-Patches konnten nicht gelesen werden")?;
        Ok(parsed.results)
    }

    /// Antivirusstand aller Geräte im Bestand.
    pub async fn antivirus_status(&self) -> Result<Vec<AntivirusEntry>> {
        if self.config.mock {
            return Ok(mock_antivirus());
        }

        #[derive(Deserialize)]
        struct QueryResponse {
            #[serde(default)]
            results: Vec<AntivirusEntry>,
        }

        let response = self
            .send(Method::GET, "/v2/queries/antivirus-status", None)
            .await?;
        let response = self.ensure_success(response).await?;
        let parsed: QueryResponse = response
            .json()
            .await
            .context("NinjaOne-Antivirusdaten konnten nicht gelesen werden")?;
        Ok(parsed.results)
    }

    pub async fn scan_patches(&self, device_id: i64, software: bool) -> Result<UpstreamOutcome> {
        let category = if software { "software" } else { "os" };
        if self.config.mock {
            return Ok(mock_outcome("PATCH_SCAN", device_id));
        }

        let response = self
            .send(
                Method::POST,
                &format!("/v2/device/{device_id}/patch/{category}/scan"),
                None,
            )
            .await?;
        self.outcome(response).await
    }

    pub async fn apply_patches(&self, device_id: i64, software: bool) -> Result<UpstreamOutcome> {
        let category = if software { "software" } else { "os" };
        if self.config.mock {
            return Ok(mock_outcome("PATCH_APPLY", device_id));
        }

        let response = self
            .send(
                Method::POST,
                &format!("/v2/device/{device_id}/patch/{category}/apply"),
                None,
            )
            .await?;
        self.outcome(response).await
    }

    /// Startet das Gerät neu. `reason` erscheint in der NinjaOne-Historie und
    /// ist der einzige Beleg dafür, wer den Neustart angeordnet hat und warum.
    pub async fn reboot_device(
        &self,
        device_id: i64,
        forced: bool,
        reason: &str,
    ) -> Result<UpstreamOutcome> {
        let mode = if forced { "FORCED" } else { "NORMAL" };
        if self.config.mock {
            return Ok(mock_outcome("REBOOT", device_id));
        }

        let response = self
            .send(
                Method::POST,
                &format!("/v2/device/{device_id}/reboot/{mode}"),
                Some(serde_json::json!({ "reason": reason })),
            )
            .await?;
        self.outcome(response).await
    }

    /// Setzt ein Wartungsfenster: NinjaOne unterdrückt währenddessen die
    /// gewählten Funktionen (Alarme, Patch-Läufe, AV-Scans, Aufgaben).
    pub async fn start_maintenance(
        &self,
        device_id: i64,
        minutes: i64,
        features: &[String],
    ) -> Result<UpstreamOutcome> {
        if self.config.mock {
            return Ok(mock_outcome("MAINTENANCE", device_id));
        }

        let start = chrono::Utc::now().timestamp_millis() as f64;
        let end = start + (minutes.max(1) * 60_000) as f64;
        let disabled = if features.is_empty() {
            vec!["ALERTS".to_string()]
        } else {
            features.to_vec()
        };

        let response = self
            .send(
                Method::PUT,
                &format!("/v2/device/{device_id}/maintenance"),
                Some(serde_json::json!({
                    "start": start,
                    "end": end,
                    "disabledFeatures": disabled,
                })),
            )
            .await?;
        self.outcome(response).await
    }

    pub async fn cancel_maintenance(&self, device_id: i64) -> Result<UpstreamOutcome> {
        if self.config.mock {
            return Ok(mock_outcome("MAINTENANCE_END", device_id));
        }

        let response = self
            .send(
                Method::DELETE,
                &format!("/v2/device/{device_id}/maintenance"),
                None,
            )
            .await?;
        self.outcome(response).await
    }

    /// Führt ein gespeichertes Script aus der NinjaOne-Bibliothek aus.
    #[allow(dead_code)]
    pub async fn run_script(&self, device_id: i64, script_id: i64) -> Result<UpstreamOutcome> {
        if self.config.mock {
            return Ok(mock_outcome("SCRIPT", device_id));
        }

        let body = serde_json::json!({ "scriptId": script_id, "runAs": "SYSTEM" });
        let response = self
            .send(
                Method::POST,
                &format!("/v2/device/{device_id}/script/run"),
                Some(body),
            )
            .await?;
        self.outcome(response).await
    }

    /// Führt ein gespeichertes Script aus der NinjaOne-Bibliothek aus.
    ///
    /// Die Public API kennt kein Inline-Script: `RunScriptRequest` erlaubt nur
    /// `type`, `id`, `uid`, `parameters` und `runAs`.
    #[allow(dead_code)]
    pub async fn run_library_script(
        &self,
        device_id: i64,
        script_id: i64,
        run_as: &str,
    ) -> Result<UpstreamOutcome> {
        if self.config.mock {
            return Ok(mock_outcome("SCRIPT", device_id));
        }

        let body = serde_json::json!({
            "type": "SCRIPT",
            "id": script_id,
            "runAs": run_as,
        });
        let response = self
            .send_as_user(
                Method::POST,
                &format!("/v2/device/{device_id}/script/run"),
                Some(body),
            )
            .await?;
        self.outcome(response).await
    }

    /// Führt eine eingebaute Aktion (z. B. "DNS-Cache löschen") per UID aus.
    pub async fn run_builtin_action(
        &self,
        device_id: i64,
        uid: &str,
        run_as: &str,
    ) -> Result<UpstreamOutcome> {
        if self.config.mock {
            return Ok(mock_outcome("ACTION", device_id));
        }

        let body = serde_json::json!({
            "type": "ACTION",
            "uid": uid,
            "runAs": run_as,
        });
        let response = self
            .send_as_user(
                Method::POST,
                &format!("/v2/device/{device_id}/script/run"),
                Some(body),
            )
            .await?;
        self.outcome(response).await
    }

    /// Verfügbare Scripts und Aktionen für ein Gerät.
    /// Grundlage für die Script-Auswahl in der Oberfläche (Phase 1).
    #[allow(dead_code)]
    pub async fn device_scripting_options(&self, device_id: i64) -> Result<serde_json::Value> {
        if self.config.mock {
            return Ok(serde_json::json!({ "categories": [], "scripts": [] }));
        }

        let response = self
            .send(
                Method::GET,
                &format!("/v2/device/{device_id}/scripting/options"),
                None,
            )
            .await?;
        let response = self.ensure_success(response).await?;
        response
            .json()
            .await
            .context("NinjaOne-Scriptoptionen konnten nicht gelesen werden")
    }

    #[allow(dead_code)]
    pub async fn list_jobs(&self, device_id: i64) -> Result<serde_json::Value> {
        if self.config.mock {
            return Ok(serde_json::json!([]));
        }

        let response = self
            .send(
                Method::GET,
                &format!("/v2/device/{device_id}/jobs"),
                None,
            )
            .await?;
        let response = self.ensure_success(response).await?;
        response
            .json()
            .await
            .context("NinjaOne-Jobs konnten nicht gelesen werden")
    }

    async fn send(
        &self,
        method: Method,
        path: &str,
        body: Option<serde_json::Value>,
    ) -> Result<reqwest::Response> {
        self.send_with(method, path, body, false).await
    }

    /// Wie `send`, nutzt aber den Token mit Benutzerkontext.
    async fn send_as_user(
        &self,
        method: Method,
        path: &str,
        body: Option<serde_json::Value>,
    ) -> Result<reqwest::Response> {
        self.send_with(method, path, body, true).await
    }

    async fn send_with(
        &self,
        method: Method,
        path: &str,
        body: Option<serde_json::Value>,
        as_user: bool,
    ) -> Result<reqwest::Response> {
        let url = format!("{}{}", self.config.base_url.trim_end_matches('/'), path);
        let mut last_error: Option<anyhow::Error> = None;

        for attempt in 0..3u32 {
            let token = if as_user {
                self.token_user().await?
            } else {
                self.token().await?
            };
            let mut request = self
                .http
                .request(method.clone(), &url)
                .bearer_auth(token);

            if let Some(body) = &body {
                request = request.json(body);
            }

            match request.send().await {
                Ok(response) if response.status() == StatusCode::UNAUTHORIZED => {
                    if as_user {
                        *self.user_token.write().await = None;
                    } else {
                        self.invalidate_token().await;
                    }
                    last_error = Some(anyhow::anyhow!(
                        "NinjaOne hat das Zugriffstoken abgelehnt"
                    ));
                }
                Ok(response)
                    if response.status() == StatusCode::TOO_MANY_REQUESTS
                        || response.status().is_server_error() =>
                {
                    let wait = retry_after(&response)
                        .unwrap_or_else(|| Duration::from_millis(400 * 2u64.pow(attempt)));
                    tracing::warn!(
                        %url,
                        status = %response.status(),
                        wait_ms = wait.as_millis(),
                        "NinjaOne: temporärer Fehler, neuer Versuch"
                    );
                    last_error = Some(anyhow::anyhow!(
                        "NinjaOne antwortete temporär mit {}",
                        response.status()
                    ));
                    tokio::time::sleep(wait).await;
                }
                Ok(response) => return Ok(response),
                Err(error) => {
                    last_error = Some(error.into());
                    tokio::time::sleep(Duration::from_millis(400 * 2u64.pow(attempt))).await;
                }
            }
        }

        Err(last_error.unwrap_or_else(|| anyhow::anyhow!("NinjaOne-Anfrage fehlgeschlagen")))
    }

    async fn ensure_success(&self, response: reqwest::Response) -> Result<reqwest::Response> {
        let status = response.status();
        if status.is_success() {
            return Ok(response);
        }

        let body = response.text().await.unwrap_or_default();
        bail!(
            "NinjaOne API antwortete mit {status}: {}",
            truncate(&body, 300)
        );
    }

    /// Wandelt eine Antwort in ein belegbares Ergebnis um: echter Status und Body.
    async fn outcome(&self, response: reqwest::Response) -> Result<UpstreamOutcome> {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();

        if !status.is_success() {
            bail!(
                "NinjaOne API antwortete mit {status}: {}",
                truncate(&body, 300)
            );
        }

        Ok(UpstreamOutcome {
            status: status.as_u16(),
            body: truncate(body.trim(), 400),
        })
    }

    async fn token(&self) -> Result<String> {
        {
            let cached = self.token.read().await;
            if let Some(token) = cached.as_ref() {
                if token.expires_at > chrono::Utc::now().timestamp() + 60 {
                    return Ok(token.value.clone());
                }
            }
        }

        let mut guard = self.token.write().await;
        if let Some(token) = guard.as_ref() {
            if token.expires_at > chrono::Utc::now().timestamp() + 60 {
                return Ok(token.value.clone());
            }
        }

        let url = format!(
            "{}/oauth/token",
            self.config.base_url.trim_end_matches('/')
        );
        let response = self
            .http
            .post(&url)
            .form(&[
                ("grant_type", "client_credentials"),
                ("client_id", self.config.client_id.as_str()),
                ("client_secret", self.config.client_secret.as_str()),
                ("scope", self.config.scope.as_str()),
            ])
            .send()
            .await
            .context("NinjaOne OAuth-Anfrage fehlgeschlagen")?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            bail!(
                "NinjaOne OAuth fehlgeschlagen ({status}): {}",
                truncate(&body, 300)
            );
        }

        #[derive(Deserialize)]
        struct TokenResponse {
            access_token: String,
            expires_in: Option<i64>,
        }

        let token: TokenResponse = response
            .json()
            .await
            .context("NinjaOne OAuth-Antwort konnte nicht gelesen werden")?;
        let expires_at = chrono::Utc::now().timestamp() + token.expires_in.unwrap_or(3600);

        *guard = Some(CachedToken {
            value: token.access_token.clone(),
            expires_at,
        });

        Ok(token.access_token)
    }

    async fn invalidate_token(&self) {
        *self.token.write().await = None;
    }
}

fn retry_after(response: &reqwest::Response) -> Option<Duration> {
    response
        .headers()
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .map(Duration::from_secs)
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

/// Prozent-Kodierung für Query-Parameter (keine zusätzliche Abhängigkeit nötig).
fn urlencode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

fn mock_activities(device_id: i64) -> Vec<DeviceActivity> {
    vec![DeviceActivity {
        id: 1,
        activity_type: Some("PATCH_MANAGEMENT".to_string()),
        status_code: Some("PATCH_MANAGEMENT_SCAN_COMPLETED".to_string()),
        status: Some("Betriebssystem Patching Scan abgeschlossen".to_string()),
        message: Some("Mock-Aktivität".to_string()),
        activity_time: Some(chrono::Utc::now().timestamp() as f64),
    }]
    .into_iter()
    .map(|mut activity| {
        activity.message = Some(format!("Mock-Aktivität für Gerät {device_id}"));
        activity
    })
    .collect()
}

fn mock_os_patches() -> Vec<OsPatch> {
    let now = chrono::Utc::now().timestamp() as f64;
    vec![
        OsPatch {
            id: "mock-patch-1".to_string(),
            name: Some("2026-10 Kumulatives Update für Windows 11 (KB5099999)".to_string()),
            severity: Some("IMPORTANT".to_string()),
            status: Some("APPROVED".to_string()),
            patch_type: Some("UPDATE_ROLLUPS".to_string()),
            kb_number: Some("KB5099999".to_string()),
            device_id: 9001,
            timestamp: Some(now),
        },
        OsPatch {
            id: "mock-patch-2".to_string(),
            name: Some("2026-09 Sicherheitsupdate für .NET (KB5088888)".to_string()),
            severity: Some("CRITICAL".to_string()),
            status: Some("REJECTED".to_string()),
            patch_type: Some("SECURITY_UPDATES".to_string()),
            kb_number: Some("KB5088888".to_string()),
            device_id: 9001,
            timestamp: Some(now),
        },
    ]
}

fn mock_software_patches() -> Vec<SoftwarePatch> {
    let now = chrono::Utc::now().timestamp() as f64;
    vec![SoftwarePatch {
        id: "mock-software-1".to_string(),
        title: Some("7-Zip 24.09".to_string()),
        impact: Some("recommended".to_string()),
        status: Some("APPROVED".to_string()),
        patch_type: Some("UPDATE".to_string()),
        product_identifier: None,
        device_id: 9001,
        timestamp: Some(now),
    }]
}

fn mock_antivirus() -> Vec<AntivirusEntry> {
    let now = chrono::Utc::now().timestamp() as f64;
    vec![
        AntivirusEntry {
            product_name: Some("Bitdefender Endpoint Security Tools Antimalware".to_string()),
            product_state: Some("ON".to_string()),
            definition_status: Some("Up-to-Date".to_string()),
            version: Some("8.0.0".to_string()),
            device_id: 9001,
            timestamp: Some(now),
        },
        AntivirusEntry {
            product_name: Some("Windows Defender".to_string()),
            product_state: Some("OFF".to_string()),
            definition_status: Some("Up-to-Date".to_string()),
            version: None,
            device_id: 9001,
            timestamp: Some(now),
        },
    ]
}

fn mock_organizations() -> Vec<Organization> {
    vec![
        Organization {
            id: 101,
            name: "Nordlicht Dental GmbH".to_string(),
            description: Some("Zahnarztpraxis, zwei Standorte".to_string()),
            node_approval_mode: Some("AUTO".to_string()),
        },
        Organization {
            id: 102,
            name: "Weber & Söhne Steuerberater".to_string(),
            description: Some("Steuerkanzlei".to_string()),
            node_approval_mode: Some("AUTO".to_string()),
        },
        Organization {
            id: 103,
            name: "Autohaus Krüger GmbH".to_string(),
            description: Some("Autohaus mit DMS".to_string()),
            node_approval_mode: Some("MANUAL".to_string()),
        },
        Organization {
            id: 104,
            name: "Kita Sonnenschein e.V.".to_string(),
            description: Some("Verein, zwei Standorte".to_string()),
            node_approval_mode: Some("AUTO".to_string()),
        },
    ]
}

fn mock_devices(organization_id: i64) -> Vec<NinjaDevice> {
    let names: &[&str] = match organization_id {
        101 => &["NDL-SRV01", "NDL-PC01", "NDL-PC02", "NDL-LT01"],
        102 => &["WBS-SRV01", "WBS-PC01", "WBS-PC02"],
        103 => &["AKG-SRV01", "AKG-PC01", "AKG-PC02", "AKG-PC03"],
        104 => &["KIS-LT01", "KIS-PC01"],
        _ => &[],
    };

    names
        .iter()
        .enumerate()
        .map(|(index, name)| NinjaDevice {
            id: organization_id * 1000 + index as i64 + 1,
            system_name: Some((*name).to_string()),
            dns_name: Some(format!("{}.local", name.to_ascii_lowercase())),
            node_class: Some(if name.contains("SRV") {
                "WINDOWS_SERVER".to_string()
            } else {
                "WINDOWS_WORKSTATION".to_string()
            }),
            offline: Some(false),
            last_contact: Some(chrono::Utc::now().timestamp() as f64),
            os: Some(serde_json::json!({ "name": "Windows" })),
            device_type: Some(if name.contains("SRV") {
                "SERVER".to_string()
            } else {
                "WORKSTATION".to_string()
            }),
            online: Some(true),
        })
        .collect()
}

fn mock_outcome(kind: &str, device_id: i64) -> UpstreamOutcome {
    UpstreamOutcome {
        status: 202,
        body: format!(
            r#"{{"id":"mock-{kind}-{device_id}","status":"QUEUED","deviceId":{device_id},"mock":true}}"#
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config(mock: bool) -> NinjaConfig {
        NinjaConfig {
            base_url: "https://eu.ninjarmm.com".to_string(),
            client_id: "test".to_string(),
            client_secret: "test".to_string(),
            scope: "monitoring management".to_string(),
            mock,
            user_client_id: String::new(),
            user_client_secret: String::new(),
            redirect_uri: "http://127.0.0.1:8080/api/v1/auth/ninjaone/callback".to_string(),
        }
    }

    #[tokio::test]
    async fn mock_client_lists_organizations() {
        let client = NinjaOneClient::new(test_config(true));
        let organizations = client.list_organizations().await.unwrap();
        assert_eq!(organizations.len(), 4);
        assert_eq!(organizations[0].id, 101);
    }

    #[tokio::test]
    async fn mock_client_lists_devices_per_organization() {
        let client = NinjaOneClient::new(test_config(true));
        let devices = client.list_devices(103).await.unwrap();
        assert_eq!(devices.len(), 4);
        assert_eq!(devices[0].system_name.as_deref(), Some("AKG-SRV01"));
    }

    #[tokio::test]
    async fn mock_jobs_are_marked_as_mock() {
        let client = NinjaOneClient::new(test_config(true));
        let outcome = client.apply_patches(42, false).await.unwrap();
        assert_eq!(outcome.status, 202);
        assert!(outcome.body.contains("\"mock\":true"));
    }
}
