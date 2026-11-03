//! Anmeldung, Sessions und Rollen.
//!
//! Passwörter liegen als Argon2id-Hash in der Datenbank. Sessions sind
//! zufällige Tokens; gespeichert wird nur ihr SHA-256-Hash. Der Token selbst
//! steckt in einem HttpOnly-Cookie und ist für JavaScript unlesbar.

use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use axum::extract::{FromRequestParts, Request, State};
use axum::http::request::Parts;
use axum::http::{header, HeaderMap, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;
use base64::Engine;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

use crate::error::{ApiError, ApiResult};
use crate::state::SharedState;
use crate::util::{audit, now_iso};

pub const SESSION_COOKIE: &str = "cockpit_session";
const SESSION_DAYS: i64 = 14;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Admin,
    Technician,
    Viewer,
}

impl Role {
    pub fn parse(value: &str) -> Option<Role> {
        match value {
            "admin" => Some(Role::Admin),
            "technician" => Some(Role::Technician),
            "viewer" => Some(Role::Viewer),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Role::Admin => "admin",
            Role::Technician => "technician",
            Role::Viewer => "viewer",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Role::Admin => "Administrator",
            Role::Technician => "Techniker",
            Role::Viewer => "Nur Lesen",
        }
    }

    /// Nur-Lesen darf keine schreibenden Methoden ausführen.
    pub fn may_write(&self) -> bool {
        !matches!(self, Role::Viewer)
    }

    pub fn is_admin(&self) -> bool {
        matches!(self, Role::Admin)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct CurrentUser {
    pub id: String,
    pub username: String,
    pub display_name: String,
    pub role: Role,
    pub role_label: String,
    pub must_change_password: bool,
}

impl CurrentUser {
    pub fn actor(&self) -> &str {
        &self.display_name
    }
}

/// Wird von Handlern als Argument verwendet: `user: CurrentUser`.
/// Antwortet mit 401, wenn keine gültige Session vorliegt.
impl FromRequestParts<SharedState> for CurrentUser {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &SharedState,
    ) -> Result<Self, Self::Rejection> {
        let token = cookie_value(&parts.headers, SESSION_COOKIE)
            .ok_or_else(|| ApiError::Unauthorized("Bitte anmelden.".to_string()))?;

        let user = lookup_session(&state.pool, &token)
            .await?
            .ok_or_else(|| ApiError::Unauthorized("Sitzung abgelaufen.".to_string()))?;

        Ok(user)
    }
}

// ---------------------------------------------------------------------------
// Passwörter
// ---------------------------------------------------------------------------

pub fn hash_password(password: &str) -> Result<String, ApiError> {
    if password.chars().count() < 8 {
        return Err(ApiError::BadRequest(
            "Das Passwort muss mindestens 8 Zeichen haben.".to_string(),
        ));
    }

    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|error| ApiError::BadRequest(format!("Passwort konnte nicht gesichert werden: {error}")))
}

pub fn verify_password(password: &str, hash: &str) -> bool {
    let Ok(parsed) = PasswordHash::new(hash) else {
        return false;
    };
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok()
}

// ---------------------------------------------------------------------------
// Sessions
// ---------------------------------------------------------------------------

fn random_token() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

fn token_hash(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Legt eine Session an und gibt den Klartext-Token zurück.
pub async fn create_session(
    pool: &PgPool,
    user_id: &str,
    user_agent: Option<&str>,
) -> Result<String, ApiError> {
    let token = random_token();
    let now = chrono::Utc::now();
    let expires = now + chrono::Duration::days(SESSION_DAYS);

    sqlx::query(
        "INSERT INTO sessions (id, user_id, token_hash, created_at, expires_at, last_seen_at, user_agent)
         VALUES ($1, $2, $3, $4, $5, $4, $6)",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(user_id)
    .bind(token_hash(&token))
    .bind(now.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
    .bind(expires.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
    .bind(user_agent)
    .execute(pool)
    .await?;

    Ok(token)
}

pub async fn lookup_session(
    pool: &PgPool,
    token: &str,
) -> Result<Option<CurrentUser>, ApiError> {
    let hash = token_hash(token);

    #[derive(sqlx::FromRow)]
    struct Row {
        id: String,
        username: String,
        display_name: String,
        role: String,
        must_change_password: bool,
    }

    let row: Option<Row> = sqlx::query_as(
        "SELECT u.id, u.username, u.display_name, u.role, u.must_change_password
         FROM sessions s
         JOIN app_users u ON u.id = s.user_id
         WHERE s.token_hash = $1 AND u.active = true AND s.expires_at > $2",
    )
    .bind(&hash)
    .bind(now_iso())
    .fetch_optional(pool)
    .await?;

    let Some(row) = row else {
        return Ok(None);
    };

    // Letzte Nutzung nachziehen, aber nur wenn nötig (spart Schreibzugriffe).
    let _ = sqlx::query("UPDATE sessions SET last_seen_at = $1 WHERE token_hash = $2")
        .bind(now_iso())
        .bind(&hash)
        .execute(pool)
        .await;

    let role = Role::parse(&row.role).unwrap_or(Role::Viewer);

    Ok(Some(CurrentUser {
        id: row.id,
        username: row.username,
        display_name: row.display_name,
        role,
        role_label: role.label().to_string(),
        must_change_password: row.must_change_password,
    }))
}

pub async fn delete_session(pool: &PgPool, token: &str) -> Result<(), ApiError> {
    sqlx::query("DELETE FROM sessions WHERE token_hash = $1")
        .bind(token_hash(token))
        .execute(pool)
        .await?;
    Ok(())
}

/// Aufräumen: abgelaufene Sessions entfernen.
/// Audit-Eintrag, wenn das Passwort per Kommandozeile zurückgesetzt wurde.
pub async fn audit_password_reset(pool: &PgPool, username: &str) -> Result<(), ApiError> {
    audit(
        pool,
        "Kommandozeile",
        "Passwort zurückgesetzt",
        Some("user"),
        None,
        &format!("Neues Passwort für {username} per --reset-admin-password gesetzt"),
    )
    .await?;
    Ok(())
}

pub async fn purge_expired(pool: &PgPool) -> Result<u64, sqlx::Error> {
    let result = sqlx::query("DELETE FROM sessions WHERE expires_at <= $1")
        .bind(now_iso())
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}

// ---------------------------------------------------------------------------
// Cookies
// ---------------------------------------------------------------------------

fn cookie_value(headers: &HeaderMap, name: &str) -> Option<String> {
    let raw = headers.get(header::COOKIE)?.to_str().ok()?;

    raw.split(';').find_map(|part| {
        let part = part.trim();
        let (key, value) = part.split_once('=')?;
        if key.trim() == name {
            Some(value.trim().to_string())
        } else {
            None
        }
    })
}

pub fn session_cookie(token: &str, secure: bool) -> String {
    let mut cookie = format!(
        "{SESSION_COOKIE}={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age={}",
        SESSION_DAYS * 86_400
    );
    if secure {
        cookie.push_str("; Secure");
    }
    cookie
}

pub fn clear_cookie(secure: bool) -> String {
    let mut cookie = format!("{SESSION_COOKIE}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0");
    if secure {
        cookie.push_str("; Secure");
    }
    cookie
}

// ---------------------------------------------------------------------------
// Middleware
// ---------------------------------------------------------------------------

/// Öffentliche Pfade, die ohne Anmeldung erreichbar bleiben.
fn is_public(path: &str) -> bool {
    matches!(
        path,
        "/api/v1/health"
            | "/api/v1/auth/login"
            | "/api/v1/auth/me"
            | "/api/v1/auth/ninjaone/callback"
    ) || !path.starts_with("/api/")
}

/// Prüft Session, Rolle und Schreibrecht für jeden API-Aufruf.
pub async fn guard(
    State(state): State<SharedState>,
    request: Request,
    next: Next,
) -> Response {
    let path = request.uri().path().to_string();

    if is_public(&path) {
        return next.run(request).await;
    }

    let Some(token) = cookie_value(request.headers(), SESSION_COOKIE) else {
        return unauthorized("Bitte anmelden.");
    };

    let user = match lookup_session(&state.pool, &token).await {
        Ok(Some(user)) => user,
        Ok(None) => return unauthorized("Sitzung abgelaufen. Bitte erneut anmelden."),
        Err(error) => {
            tracing::error!(?error, "Session-Prüfung fehlgeschlagen");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Interner Fehler").into_response();
        }
    };

    let method = request.method().clone();
    let is_auth_path = path.starts_with("/api/v1/auth/");
    let is_write = !matches!(method, axum::http::Method::GET | axum::http::Method::HEAD);

    // Nur-Lesen: alles außer GET/HEAD ist gesperrt – die eigene
    // Anmeldung und der Passwortwechsel bleiben aber erlaubt.
    if !user.role.may_write() && is_write && !is_auth_path {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({
                "error": "Diese Aktion ist für Nur-Lesen-Benutzer gesperrt."
            })),
        )
            .into_response();
    }

    // Benutzerverwaltung nur für Administratoren.
    if path.starts_with("/api/v1/users") && !user.role.is_admin() {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({
                "error": "Benutzerverwaltung ist Administratoren vorbehalten."
            })),
        )
            .into_response();
    }

    // Passwortwechsel erzwingen: alle anderen Schreibzugriffe blocken.
    if user.must_change_password && is_write && !is_auth_path {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({
                "error": "Bitte zuerst das Passwort ändern.",
                "must_change_password": true
            })),
        )
            .into_response();
    }

    next.run(request).await
}

fn unauthorized(message: &str) -> Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(serde_json::json!({ "error": message })),
    )
        .into_response()
}

// ---------------------------------------------------------------------------
// Startbenutzer
// ---------------------------------------------------------------------------

/// Legt beim ersten Start einen Administrator an und gibt das Passwort
/// einmalig im Log aus.
pub async fn ensure_admin(pool: &PgPool) -> anyhow::Result<()> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM app_users")
        .fetch_one(pool)
        .await?;

    if count > 0 {
        return Ok(());
    }

    let username = std::env::var("ADMIN_USERNAME").unwrap_or_else(|_| "admin".to_string());
    let display_name = std::env::var("ADMIN_DISPLAY_NAME")
        .unwrap_or_else(|_| "Administrator".to_string());

    let password = std::env::var("ADMIN_PASSWORD").unwrap_or_else(|_| {
        let mut bytes = [0u8; 9];
        rand::thread_rng().fill_bytes(&mut bytes);
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
    });

    let hash = hash_password(&password)
        .map_err(|error| anyhow::anyhow!("Startpasswort ungültig: {error}"))?;

    sqlx::query(
        "INSERT INTO app_users
         (id, username, display_name, password_hash, role, active, must_change_password, created_at)
         VALUES ($1, $2, $3, $4, 'admin', true, $5, $6)",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(&username)
    .bind(&display_name)
    .bind(hash)
    .bind(std::env::var("ADMIN_PASSWORD").is_err())
    .bind(now_iso())
    .execute(pool)
    .await?;

    if std::env::var("ADMIN_PASSWORD").is_err() {
        tracing::warn!(
            username = %username,
            password = %password,
            "Erster Administrator angelegt. Passwort jetzt notieren und bei der ersten Anmeldung ändern."
        );
    } else {
        tracing::info!(username = %username, "Erster Administrator aus ADMIN_PASSWORD angelegt");
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Handler-Bausteine
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub user: CurrentUser,
    pub must_change_password: bool,
}

pub async fn authenticate(
    pool: &PgPool,
    username: &str,
    password: &str,
    user_agent: Option<&str>,
) -> ApiResult<(CurrentUser, String)> {
    #[derive(sqlx::FromRow)]
    struct Row {
        id: String,
        username: String,
        display_name: String,
        password_hash: String,
        role: String,
        active: bool,
        must_change_password: bool,
    }

    let entered = username.trim();

    // Erst der Benutzername, dann – falls eindeutig – der Anzeigename.
    // So klappt auch "Administrator" statt "admin".
    let mut row: Option<Row> = sqlx::query_as(
        "SELECT id, username, display_name, password_hash, role, active, must_change_password
         FROM app_users WHERE username = $1",
    )
    .bind(entered)
    .fetch_optional(pool)
    .await?;

    if row.is_none() {
        let by_display: Vec<Row> = sqlx::query_as(
            "SELECT id, username, display_name, password_hash, role, active, must_change_password
             FROM app_users WHERE lower(display_name) = lower($1)",
        )
        .bind(entered)
        .fetch_all(pool)
        .await?;

        // Nur bei eindeutiger Zuordnung – bei Dubletten bleibt es beim Benutzernamen.
        if by_display.len() == 1 {
            row = by_display.into_iter().next();
        }
    }

    // Bewusst gleiche Meldung für unbekannten Benutzer und falsches Passwort.
    let Some(row) = row else {
        return Err(ApiError::Unauthorized(
            "Benutzername oder Passwort ist falsch.".to_string(),
        ));
    };

    if !row.active {
        return Err(ApiError::Unauthorized(
            "Dieses Konto ist deaktiviert.".to_string(),
        ));
    }

    if !verify_password(password, &row.password_hash) {
        return Err(ApiError::Unauthorized(
            "Benutzername oder Passwort ist falsch.".to_string(),
        ));
    }

    let role = Role::parse(&row.role).unwrap_or(Role::Viewer);
    let token = create_session(pool, &row.id, user_agent).await?;

    sqlx::query("UPDATE app_users SET last_login_at = $1 WHERE id = $2")
        .bind(now_iso())
        .bind(&row.id)
        .execute(pool)
        .await?;

    audit(
        pool,
        &row.display_name,
        "Anmeldung",
        Some("user"),
        Some(&row.id),
        "Benutzer angemeldet",
    )
    .await?;

    Ok((
        CurrentUser {
            id: row.id,
            username: row.username,
            display_name: row.display_name,
            role,
            role_label: role.label().to_string(),
            must_change_password: row.must_change_password,
        },
        token,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_roundtrip() {
        let hash = hash_password("geheim123").unwrap();
        assert!(verify_password("geheim123", &hash));
        assert!(!verify_password("falsch", &hash));
    }

    #[test]
    fn short_password_rejected() {
        assert!(hash_password("kurz").is_err());
    }

    #[test]
    fn token_hashes_differ() {
        let a = token_hash("abc");
        let b = token_hash("abd");
        assert_ne!(a, b);
        assert_eq!(a.len(), 64);
    }

    #[test]
    fn roles_parse_and_permissions() {
        assert_eq!(Role::parse("admin"), Some(Role::Admin));
        assert_eq!(Role::parse("quatsch"), None);
        assert!(!Role::Viewer.may_write());
        assert!(Role::Technician.may_write());
        assert!(Role::Admin.is_admin());
    }
}
