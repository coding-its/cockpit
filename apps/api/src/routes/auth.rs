//! Endpunkte für Anmeldung und Benutzerverwaltung.

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::auth::{
    self, clear_cookie, hash_password, session_cookie, CurrentUser, LoginRequest, LoginResponse,
    Role, SESSION_COOKIE,
};
use crate::error::{ApiError, ApiResult};
use crate::state::SharedState;
use crate::util::{audit, now_iso};

pub fn router() -> Router<SharedState> {
    Router::new()
        .route("/api/v1/auth/login", post(login))
        .route("/api/v1/auth/logout", post(logout))
        .route("/api/v1/auth/me", get(me))
        .route("/api/v1/auth/password", post(change_own_password))
        .route("/api/v1/users", get(list_users).post(create_user))
        .route("/api/v1/users/{id}", axum::routing::patch(update_user))
        .route("/api/v1/users/{id}/password", post(reset_password))
}

fn secure_cookies() -> bool {
    matches!(
        std::env::var("SESSION_COOKIE_SECURE").as_deref(),
        Ok("1") | Ok("true") | Ok("yes")
    )
}

fn with_cookie(body: impl IntoResponse, cookie: String) -> Response {
    let mut response = body.into_response();
    if let Ok(value) = cookie.parse() {
        response
            .headers_mut()
            .append(axum::http::header::SET_COOKIE, value);
    }
    response
}

// ---------------------------------------------------------------------------
// Anmeldung
// ---------------------------------------------------------------------------

async fn login(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(payload): Json<LoginRequest>,
) -> ApiResult<Response> {
    let agent = headers
        .get(axum::http::header::USER_AGENT)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.chars().take(180).collect::<String>());

    let (user, token) = auth::authenticate(
        &state.pool,
        &payload.username,
        &payload.password,
        agent.as_deref(),
    )
    .await?;

    let cookie = session_cookie(&token, secure_cookies());

    Ok(with_cookie(
        Json(LoginResponse {
            must_change_password: user.must_change_password,
            user,
        }),
        cookie,
    ))
}

async fn logout(State(state): State<SharedState>, headers: HeaderMap) -> ApiResult<Response> {
    if let Some(token) = headers
        .get(axum::http::header::COOKIE)
        .and_then(|value| value.to_str().ok())
        .and_then(|raw| {
            raw.split(';').find_map(|part| {
                let (key, value) = part.trim().split_once('=')?;
                (key.trim() == SESSION_COOKIE).then(|| value.trim().to_string())
            })
        })
    {
        // Wer war das? Für den Audit-Eintrag vor dem Löschen ermitteln.
        let name = auth::lookup_session(&state.pool, &token)
            .await
            .ok()
            .flatten()
            .map(|user| user.display_name);

        let _ = auth::delete_session(&state.pool, &token).await;

        if let Some(name) = name {
            let _ = audit(
                &state.pool,
                &name,
                "Abmeldung",
                Some("user"),
                None,
                "Benutzer abgemeldet",
            )
            .await;
        }
    }

    Ok(with_cookie(
        StatusCode::NO_CONTENT,
        clear_cookie(secure_cookies()),
    ))
}

async fn me(State(state): State<SharedState>, headers: HeaderMap) -> ApiResult<Json<MeResponse>> {
    let token = headers
        .get(axum::http::header::COOKIE)
        .and_then(|value| value.to_str().ok())
        .and_then(|raw| {
            raw.split(';').find_map(|part| {
                let (key, value) = part.trim().split_once('=')?;
                (key.trim() == SESSION_COOKIE).then(|| value.trim().to_string())
            })
        });

    let Some(token) = token else {
        return Ok(Json(MeResponse { user: None }));
    };

    let user = auth::lookup_session(&state.pool, &token).await?;
    Ok(Json(MeResponse { user }))
}

#[derive(Debug, Serialize)]
pub struct MeResponse {
    pub user: Option<CurrentUser>,
}

#[derive(Debug, Deserialize)]
pub struct ChangePasswordRequest {
    pub current_password: String,
    pub new_password: String,
}

async fn change_own_password(
    State(state): State<SharedState>,
    user: CurrentUser,
    Json(payload): Json<ChangePasswordRequest>,
) -> ApiResult<Json<CurrentUser>> {
    #[derive(sqlx::FromRow)]
    struct Row {
        password_hash: String,
    }

    let row: Option<Row> =
        sqlx::query_as("SELECT password_hash FROM app_users WHERE id = $1")
            .bind(&user.id)
            .fetch_optional(&state.pool)
            .await?;

    let Some(row) = row else {
        return Err(ApiError::NotFound("Benutzer nicht gefunden".to_string()));
    };

    if !auth::verify_password(&payload.current_password, &row.password_hash) {
        return Err(ApiError::BadRequest(
            "Das aktuelle Passwort ist falsch.".to_string(),
        ));
    }

    if payload.new_password == payload.current_password {
        return Err(ApiError::BadRequest(
            "Das neue Passwort muss sich vom bisherigen unterscheiden.".to_string(),
        ));
    }

    let hash = hash_password(&payload.new_password)?;

    sqlx::query("UPDATE app_users SET password_hash = $1, must_change_password = false WHERE id = $2")
        .bind(&hash)
        .bind(&user.id)
        .execute(&state.pool)
        .await?;

    audit(
        &state.pool,
        &user.display_name,
        "Passwort geändert",
        Some("user"),
        Some(&user.id),
        "Eigenes Passwort geändert",
    )
    .await?;

    Ok(Json(CurrentUser {
        must_change_password: false,
        ..user
    }))
}

// ---------------------------------------------------------------------------
// Benutzerverwaltung
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct UserRow {
    pub id: String,
    pub username: String,
    pub display_name: String,
    pub role: String,
    pub active: bool,
    pub must_change_password: bool,
    pub created_at: String,
    pub last_login_at: Option<String>,
}

async fn list_users(State(state): State<SharedState>) -> ApiResult<Json<Vec<UserRow>>> {
    let users = sqlx::query_as::<_, UserRow>(
        "SELECT id, username, display_name, role, active, must_change_password, created_at, last_login_at
         FROM app_users ORDER BY active DESC, display_name",
    )
    .fetch_all(&state.pool)
    .await?;

    Ok(Json(users))
}

#[derive(Debug, Deserialize)]
pub struct CreateUserRequest {
    pub username: String,
    pub display_name: String,
    pub password: String,
    pub role: String,
}

async fn create_user(
    State(state): State<SharedState>,
    user: CurrentUser,
    Json(payload): Json<CreateUserRequest>,
) -> ApiResult<Json<UserRow>> {
    let username = payload.username.trim().to_lowercase();
    if username.len() < 3 {
        return Err(ApiError::BadRequest(
            "Der Benutzername braucht mindestens 3 Zeichen.".to_string(),
        ));
    }

    if !username
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_')
    {
        return Err(ApiError::BadRequest(
            "Benutzername darf nur Buchstaben, Ziffern, Punkt, Bindestrich und Unterstrich enthalten."
                .to_string(),
        ));
    }

    let display_name = payload.display_name.trim();
    if display_name.is_empty() {
        return Err(ApiError::BadRequest("Bitte einen Anzeigenamen angeben.".to_string()));
    }

    let role = Role::parse(&payload.role)
        .ok_or_else(|| ApiError::BadRequest("Rolle muss admin, technician oder viewer sein.".to_string()))?;

    let exists: Option<String> =
        sqlx::query_scalar("SELECT id FROM app_users WHERE username = $1")
            .bind(&username)
            .fetch_optional(&state.pool)
            .await?;

    if exists.is_some() {
        return Err(ApiError::Conflict(
            "Dieser Benutzername ist bereits vergeben.".to_string(),
        ));
    }

    let hash = hash_password(&payload.password)?;
    let id = Uuid::new_v4().to_string();

    sqlx::query(
        "INSERT INTO app_users
         (id, username, display_name, password_hash, role, active, must_change_password, created_at)
         VALUES ($1, $2, $3, $4, $5, true, true, $6)",
    )
    .bind(&id)
    .bind(&username)
    .bind(display_name)
    .bind(hash)
    .bind(role.as_str())
    .bind(now_iso())
    .execute(&state.pool)
    .await?;

    audit(
        &state.pool,
        user.actor(),
        "Benutzer angelegt",
        Some("user"),
        Some(&id),
        &format!("{display_name} ({}) als {}", username, role.label()),
    )
    .await?;

    let row = fetch_user(&state.pool, &id).await?;
    Ok(Json(row))
}

#[derive(Debug, Deserialize)]
pub struct UpdateUserRequest {
    pub display_name: Option<String>,
    pub role: Option<String>,
    pub active: Option<bool>,
}

async fn update_user(
    State(state): State<SharedState>,
    user: CurrentUser,
    Path(id): Path<String>,
    Json(payload): Json<UpdateUserRequest>,
) -> ApiResult<Json<UserRow>> {
    let target = fetch_user(&state.pool, &id).await?;

    if let Some(display_name) = payload.display_name.as_deref() {
        let display_name = display_name.trim();
        if display_name.is_empty() {
            return Err(ApiError::BadRequest("Der Anzeigename darf nicht leer sein.".to_string()));
        }
        sqlx::query("UPDATE app_users SET display_name = $1 WHERE id = $2")
            .bind(display_name)
            .bind(&id)
            .execute(&state.pool)
            .await?;
    }

    if let Some(role_value) = payload.role.as_deref() {
        let role = Role::parse(role_value)
            .ok_or_else(|| ApiError::BadRequest("Unbekannte Rolle.".to_string()))?;

        // Verhindern, dass der letzte Administrator sich selbst degradiert.
        if target.role == "admin" && !role.is_admin() {
            ensure_not_last_admin(&state.pool, &id).await?;
        }

        sqlx::query("UPDATE app_users SET role = $1 WHERE id = $2")
            .bind(role.as_str())
            .bind(&id)
            .execute(&state.pool)
            .await?;
    }

    if let Some(active) = payload.active {
        if !active {
            if id == user.id {
                return Err(ApiError::BadRequest(
                    "Das eigene Konto kann nicht deaktiviert werden.".to_string(),
                ));
            }
            if target.role == "admin" {
                ensure_not_last_admin(&state.pool, &id).await?;
            }
            // Deaktivieren beendet laufende Sessions.
            sqlx::query("DELETE FROM sessions WHERE user_id = $1")
                .bind(&id)
                .execute(&state.pool)
                .await?;
        }

        sqlx::query("UPDATE app_users SET active = $1 WHERE id = $2")
            .bind(active)
            .bind(&id)
            .execute(&state.pool)
            .await?;
    }

    audit(
        &state.pool,
        user.actor(),
        "Benutzer geändert",
        Some("user"),
        Some(&id),
        &format!("Konto {} aktualisiert", target.username),
    )
    .await?;

    let row = fetch_user(&state.pool, &id).await?;
    Ok(Json(row))
}

#[derive(Debug, Deserialize)]
pub struct ResetPasswordRequest {
    pub new_password: String,
}

async fn reset_password(
    State(state): State<SharedState>,
    user: CurrentUser,
    Path(id): Path<String>,
    Json(payload): Json<ResetPasswordRequest>,
) -> ApiResult<StatusCode> {
    let target = fetch_user(&state.pool, &id).await?;

    let hash = hash_password(&payload.new_password)?;

    sqlx::query(
        "UPDATE app_users SET password_hash = $1, must_change_password = true WHERE id = $2",
    )
    .bind(&hash)
    .bind(&id)
    .execute(&state.pool)
    .await?;

    // Alle bestehenden Sessions des Kontos beenden.
    sqlx::query("DELETE FROM sessions WHERE user_id = $1")
        .bind(&id)
        .execute(&state.pool)
        .await?;

    audit(
        &state.pool,
        user.actor(),
        "Passwort zurückgesetzt",
        Some("user"),
        Some(&id),
        &format!("Passwort für {} neu gesetzt", target.username),
    )
    .await?;

    Ok(StatusCode::NO_CONTENT)
}

async fn ensure_not_last_admin(pool: &PgPool, id: &str) -> ApiResult<()> {
    let admins: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM app_users WHERE role = 'admin' AND active = true AND id != $1",
    )
    .bind(id)
    .fetch_one(pool)
    .await?;

    if admins == 0 {
        return Err(ApiError::Conflict(
            "Der letzte aktive Administrator kann nicht herabgestuft oder deaktiviert werden."
                .to_string(),
        ));
    }

    Ok(())
}

async fn fetch_user(pool: &PgPool, id: &str) -> ApiResult<UserRow> {
    sqlx::query_as::<_, UserRow>(
        "SELECT id, username, display_name, role, active, must_change_password, created_at, last_login_at
         FROM app_users WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| ApiError::NotFound(format!("Benutzer {id} nicht gefunden")))
}
