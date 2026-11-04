//! WordPress-Kunden und ihre Websites.
//!
//! Bewusst getrennt von den NinjaOne-Kunden: andere Stammdaten, eigene
//! Zugangsdaten, eigene Zeiterfassung. Eine Website hat Aktionen
//! (Health, Plugin-Update, Core-Update) statt eines Wartungslaufs.

use std::time::Instant;

use axum::extract::{Path, State};
use axum::routing::{get, patch, post};
use axum::http::StatusCode;
use axum::{Json, Router};
use sqlx::PgPool;
use uuid::Uuid;

use crate::auth::CurrentUser;
use crate::error::{ApiError, ApiResult};
use crate::models::*;
use crate::queries;
use crate::state::SharedState;
use crate::util::{audit, current_month, now_iso, parse_timestamp};
use crate::wordpress::{WpAgent, WpAgentConfig};

pub fn router(_state: SharedState) -> Router<SharedState> {
    Router::new()
        .route("/api/v1/web-clients", get(list_clients).post(create_client))
        .route(
            "/api/v1/web-clients/{id}",
            get(get_client).patch(update_client).delete(delete_client),
        )
        .route("/api/v1/web-clients/{id}/sites", post(create_site))
        .route(
            "/api/v1/web-clients/{id}/statement",
            get(web_statement),
        )
        .route(
            "/api/v1/web-clients/{id}/statement/billed",
            post(mark_web_statement),
        )
        .route(
            "/api/v1/web-sites/{id}",
            axum::routing::patch(update_site).delete(delete_site),
        )
        .route("/api/v1/web-sites/{id}/actions", get(list_site_actions).post(run_action))
        .route("/api/v1/web-actions/{id}", get(get_action))
        .route("/api/v1/web-timer/start", post(start_timer))
        .route("/api/v1/web-time-entries", post(create_manual_entry))
        .route(
            "/api/v1/time-entries/{id}/comment",
            patch(update_comment),
        )
}

// ---------------------------------------------------------------------------
// Web-Kunden
// ---------------------------------------------------------------------------

pub async fn list_clients(
    State(state): State<SharedState>,
) -> ApiResult<Json<Vec<WebClientSummary>>> {
    let month = current_month();

    let clients = sqlx::query_as::<_, WebClientSummary>(
        r#"
        SELECT
            w.id, w.name, w.short_code, w.contact_name, w.contact_email, w.phone,
            w.website_url, w.hourly_rate,
            (SELECT COUNT(*) FROM sites s WHERE s.web_client_id = w.id) AS site_count,
            COALESCE((SELECT SUM(s.plugin_updates) FROM sites s WHERE s.web_client_id = w.id), 0)::bigint AS plugin_updates,
            COALESCE((
                SELECT SUM(te.duration_seconds) FROM time_entries te
                WHERE te.web_client_id = w.id AND te.billable = true
                  AND substr(te.started_at, 1, 7) = $1
            ), 0)::double precision / 3600.0 AS month_hours,
            COALESCE((
                SELECT SUM(te.duration_seconds) FROM time_entries te
                WHERE te.web_client_id = w.id AND te.billable = true
            ), 0)::double precision / 3600.0 AS total_hours,
            (SELECT COUNT(*) FROM sites s WHERE s.web_client_id = w.id
                AND s.status IN ('warning', 'critical')) AS issues,
            (SELECT MAX(te.started_at) FROM time_entries te WHERE te.web_client_id = w.id) AS last_activity
        FROM web_clients w
        WHERE w.active = true
        ORDER BY w.name
        "#,
    )
    .bind(&month)
    .fetch_all(&state.pool)
    .await?;

    Ok(Json(clients))
}

pub async fn create_client(
    State(state): State<SharedState>,
    user: CurrentUser,
    Json(payload): Json<CreateWebClientRequest>,
) -> ApiResult<Json<WebClient>> {
    let name = payload.name.trim();
    if name.is_empty() {
        return Err(ApiError::BadRequest("Bitte einen Namen angeben.".to_string()));
    }

    let short_code = payload
        .short_code
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_uppercase())
        .unwrap_or_else(|| derive_short_code(name));

    let exists: Option<String> =
        sqlx::query_scalar("SELECT id FROM web_clients WHERE short_code = $1")
            .bind(&short_code)
            .fetch_optional(&state.pool)
            .await?;

    if exists.is_some() {
        return Err(ApiError::Conflict(format!(
            "Das Kürzel {short_code} ist bereits vergeben."
        )));
    }

    let id = Uuid::new_v4().to_string();
    let now = now_iso();

    sqlx::query(
        "INSERT INTO web_clients
         (id, name, short_code, contact_name, contact_email, phone, website_url, notes,
          hourly_rate, active, created_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, true, $10)",
    )
    .bind(&id)
    .bind(name)
    .bind(&short_code)
    .bind(payload.contact_name.as_deref())
    .bind(payload.contact_email.as_deref())
    .bind(payload.phone.as_deref())
    .bind(payload.website_url.as_deref())
    .bind(payload.notes.as_deref())
    .bind(payload.hourly_rate.unwrap_or(crate::billing::DEFAULT_HOURLY_RATE))
    .bind(&now)
    .execute(&state.pool)
    .await?;

    audit(
        &state.pool,
        user.actor(),
        "Web-Kunde angelegt",
        Some("web_client"),
        Some(&id),
        name,
    )
    .await?;

    Ok(Json(fetch_client(&state.pool, &id).await?))
}

/// Aufstellung der noch nicht abgerechneten Zeiten eines Web-Kunden.
pub async fn web_statement(
    State(state): State<SharedState>,
    user: CurrentUser,
    Path(id): Path<String>,
) -> ApiResult<Json<crate::statement::Statement>> {
    let billing = crate::settings::billing(&state.pool).await;

    let mut statement = crate::statement::for_web_client(&state.pool, &id, &billing)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Web-Kunde {id} nicht gefunden")))?;

    statement.generated_by = user.display_name.clone();

    Ok(Json(statement))
}

/// Markiert die gedruckten Zeiten eines Web-Kunden als abgerechnet.
pub async fn mark_web_statement(
    State(state): State<SharedState>,
    user: CurrentUser,
    Path(id): Path<String>,
    Json(payload): Json<BilledRequest>,
) -> ApiResult<Json<BilledResponse>> {
    let marked =
        crate::statement::mark_billed(&state.pool, "web", &id, &payload.entry_ids).await?;

    if marked > 0 {
        audit(
            &state.pool,
            user.actor(),
            "Zeiten als abgerechnet markiert",
            Some("web_client"),
            Some(&id),
            &format!("{marked} Einträge"),
        )
        .await?;
    }

    Ok(Json(BilledResponse { marked }))
}

pub async fn get_client(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> ApiResult<Json<WebClientDetail>> {
    let client = fetch_client(&state.pool, &id).await?;

    let sites = sqlx::query_as::<_, Site>(
        "SELECT * FROM sites WHERE web_client_id = $1 ORDER BY environment, label, url",
    )
    .bind(&id)
    .fetch_all(&state.pool)
    .await?;

    let actions = sqlx::query_as::<_, WebAction>(
        "SELECT * FROM web_actions WHERE web_client_id = $1 ORDER BY created_at DESC LIMIT 25",
    )
    .bind(&id)
    .fetch_all(&state.pool)
    .await?;

    let time_entries = queries::time_entries(&state.pool, None, Some(&id), None, 50).await?;

    let month = current_month();
    let month_hours: Option<f64> = sqlx::query_scalar(
        "SELECT COALESCE(SUM(duration_seconds), 0)::double precision / 3600.0 FROM time_entries
         WHERE web_client_id = $1 AND billable = true AND substr(started_at, 1, 7) = $2",
    )
    .bind(&id)
    .bind(&month)
    .fetch_one(&state.pool)
    .await?;

    let total_hours: Option<f64> = sqlx::query_scalar(
        "SELECT COALESCE(SUM(duration_seconds), 0)::double precision / 3600.0 FROM time_entries
         WHERE web_client_id = $1 AND billable = true",
    )
    .bind(&id)
    .fetch_one(&state.pool)
    .await?;

    Ok(Json(WebClientDetail {
        client,
        sites,
        actions,
        time_entries,
        month_hours: month_hours.unwrap_or(0.0),
        total_hours: total_hours.unwrap_or(0.0),
    }))
}

pub async fn update_client(
    State(state): State<SharedState>,
    user: CurrentUser,
    Path(id): Path<String>,
    Json(payload): Json<UpdateWebClientRequest>,
) -> ApiResult<Json<WebClient>> {
    let exists: Option<String> = sqlx::query_scalar("SELECT id FROM web_clients WHERE id = $1")
        .bind(&id)
        .fetch_optional(&state.pool)
        .await?;

    if exists.is_none() {
        return Err(ApiError::NotFound(format!("Web-Kunde {id} nicht gefunden")));
    }

    if let Some(name) = payload.name.as_deref() {
        let name = name.trim();
        if name.is_empty() {
            return Err(ApiError::BadRequest("Der Name darf nicht leer sein.".to_string()));
        }
        sqlx::query("UPDATE web_clients SET name = $1 WHERE id = $2")
            .bind(name)
            .bind(&id)
            .execute(&state.pool)
            .await?;
    }

    if let Some(code) = payload.short_code.as_deref() {
        let code = code.trim().to_uppercase();
        if !code.is_empty() {
            let taken: Option<String> =
                sqlx::query_scalar("SELECT id FROM web_clients WHERE short_code = $1 AND id != $2")
                    .bind(&code)
                    .bind(&id)
                    .fetch_optional(&state.pool)
                    .await?;
            if taken.is_some() {
                return Err(ApiError::Conflict(format!(
                    "Das Kürzel {code} ist bereits vergeben."
                )));
            }
            sqlx::query("UPDATE web_clients SET short_code = $1 WHERE id = $2")
                .bind(&code)
                .bind(&id)
                .execute(&state.pool)
                .await?;
        }
    }

    if payload.contact_name.is_some() {
        sqlx::query("UPDATE web_clients SET contact_name = $1 WHERE id = $2")
            .bind(payload.contact_name.as_deref())
            .bind(&id)
            .execute(&state.pool)
            .await?;
    }
    if payload.contact_email.is_some() {
        sqlx::query("UPDATE web_clients SET contact_email = $1 WHERE id = $2")
            .bind(payload.contact_email.as_deref())
            .bind(&id)
            .execute(&state.pool)
            .await?;
    }
    if payload.phone.is_some() {
        sqlx::query("UPDATE web_clients SET phone = $1 WHERE id = $2")
            .bind(payload.phone.as_deref())
            .bind(&id)
            .execute(&state.pool)
            .await?;
    }
    if payload.website_url.is_some() {
        sqlx::query("UPDATE web_clients SET website_url = $1 WHERE id = $2")
            .bind(payload.website_url.as_deref())
            .bind(&id)
            .execute(&state.pool)
            .await?;
    }
    if payload.notes.is_some() {
        sqlx::query("UPDATE web_clients SET notes = $1 WHERE id = $2")
            .bind(payload.notes.as_deref())
            .bind(&id)
            .execute(&state.pool)
            .await?;
    }
    if let Some(rate) = payload.hourly_rate {
        sqlx::query("UPDATE web_clients SET hourly_rate = $1 WHERE id = $2")
            .bind(rate)
            .bind(&id)
            .execute(&state.pool)
            .await?;
    }
    if let Some(active) = payload.active {
        sqlx::query("UPDATE web_clients SET active = $1 WHERE id = $2")
            .bind(active)
            .bind(&id)
            .execute(&state.pool)
            .await?;
    }

    audit(
        &state.pool,
        user.actor(),
        "Web-Kunde geändert",
        Some("web_client"),
        Some(&id),
        "Stammdaten aktualisiert",
    )
    .await?;

    Ok(Json(fetch_client(&state.pool, &id).await?))
}

// ---------------------------------------------------------------------------
// Websites
// ---------------------------------------------------------------------------

pub async fn create_site(
    State(state): State<SharedState>,
    user: CurrentUser,
    Path(id): Path<String>,
    Json(payload): Json<CreateWebSiteRequest>,
) -> ApiResult<Json<Site>> {
    fetch_client(&state.pool, &id).await?;

    let raw = payload.url.trim();
    if raw.is_empty() {
        return Err(ApiError::BadRequest("Bitte eine Internetadresse angeben.".to_string()));
    }

    let url = normalize_url(raw)?;

    let exists: Option<String> = sqlx::query_scalar("SELECT id FROM sites WHERE url = $1")
        .bind(&url)
        .fetch_optional(&state.pool)
        .await?;

    if exists.is_some() {
        return Err(ApiError::Conflict(
            "Diese Adresse ist bereits als Website hinterlegt.".to_string(),
        ));
    }

    let environment = payload.environment.as_deref().unwrap_or("prod");
    if !matches!(environment, "prod" | "staging" | "local") {
        return Err(ApiError::BadRequest(
            "Umgebung muss prod, staging oder local sein.".to_string(),
        ));
    }

    let site_id = Uuid::new_v4().to_string();
    let now = now_iso();

    sqlx::query(
        "INSERT INTO sites
         (id, customer_id, web_client_id, url, label, environment, agent_mode,
          plugin_updates, core_update, status, notes,
          agent_user, agent_password, agent_secret,
          login_url, login_user, login_password, login_note, backup_confirmed)
         VALUES ($1, NULL, $2, $3, $4, $5, 'rest', 0, 0, 'unknown', $6,
                 $7, $8, $9, $10, $11, $12, $13, 0)",
    )
    .bind(&site_id)
    .bind(&id)
    .bind(&url)
    .bind(payload.label.as_deref().filter(|v| !v.trim().is_empty()))
    .bind(environment)
    .bind(payload.notes.as_deref())
    .bind(payload.agent_user.as_deref())
    .bind(payload.agent_password.as_deref())
    .bind(payload.agent_secret.as_deref())
    .bind(payload.login_url.as_deref())
    .bind(payload.login_user.as_deref())
    .bind(payload.login_password.as_deref())
    .bind(payload.login_note.as_deref())
    .execute(&state.pool)
    .await?;

    audit(
        &state.pool,
        user.actor(),
        "Website angelegt",
        Some("site"),
        Some(&site_id),
        &format!("{url} · Web-Kunde {id} · {environment} · {now}"),
    )
    .await?;

    let site = sqlx::query_as::<_, Site>("SELECT * FROM sites WHERE id = $1")
        .bind(&site_id)
        .fetch_one(&state.pool)
        .await?;

    Ok(Json(site))
}

/// Löscht einen Web-Kunden samt Websites und Agent-Zugängen.
///
/// Zeiten sind der Haken: sie hängen per Fremdschlüssel am Kunden und würden
/// mitgelöscht. Abrechnungsdaten dürfen nicht verschwinden, weil jemand
/// Stammdaten aufräumt – deshalb wird bei vorhandenen Zeiten abgelehnt und auf
/// „deaktivieren" verwiesen.
pub async fn delete_client(
    State(state): State<SharedState>,
    user: CurrentUser,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    let client: Option<(String, String, String)> =
        sqlx::query_as("SELECT name, short_code, CAST(active AS TEXT) FROM web_clients WHERE id = $1")
            .bind(&id)
            .fetch_optional(&state.pool)
            .await?;

    let Some((name, short_code, _)) = client else {
        return Err(ApiError::NotFound(format!("Web-Kunde {id} nicht gefunden")));
    };

    let entries: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM time_entries WHERE web_client_id = $1")
        .bind(&id)
        .fetch_one(&state.pool)
        .await?;

    if entries > 0 {
        return Err(ApiError::Conflict(format!(
            "{name} hat {entries} Zeiteinträge. Löschen würde die Abrechnung verändern. Nutze stattdessen „deaktivieren“ – der Kunde verschwindet aus den Listen, die Zeiten bleiben erhalten."
        )));
    }

    let sites: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sites WHERE web_client_id = $1")
        .bind(&id)
        .fetch_one(&state.pool)
        .await?;
    let actions: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM web_actions WHERE web_client_id = $1",
    )
    .bind(&id)
    .fetch_one(&state.pool)
    .await?;

    // Sites und Aktionen hängen per ON DELETE CASCADE am Kunden.
    sqlx::query("DELETE FROM web_clients WHERE id = $1")
        .bind(&id)
        .execute(&state.pool)
        .await?;

    audit(
        &state.pool,
        user.actor(),
        "Web-Kunde gelöscht",
        Some("web_client"),
        Some(&id),
        &format!("{name} ({short_code}) · {sites} Websites, {actions} Aktionen mitentfernt"),
    )
    .await?;

    Ok(StatusCode::NO_CONTENT)
}

/// Ändert eine Website des Web-Kunden.
///
/// Felder, die nicht mitgeschickt werden, bleiben unverändert. Explizit `null`
/// löscht einen Wert. Bei Agent-Zugangsdaten heißt „nicht mitgeschickt": der
/// gespeicherte Wert bleibt – so kann man eine Notiz ändern, ohne das Secret
/// erneut einzutippen.
pub async fn update_site(
    State(state): State<SharedState>,
    user: CurrentUser,
    Path(id): Path<String>,
    Json(payload): Json<UpdateWebSiteRequest>,
) -> ApiResult<Json<Site>> {
    #[allow(clippy::type_complexity)]
    let current: Option<(
        String,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
    )> = sqlx::query_as(
        "SELECT url, environment, label, notes, agent_user, agent_secret,
                login_url, login_user, login_note, login_password
         FROM sites WHERE id = $1",
    )
    .bind(&id)
    .fetch_optional(&state.pool)
    .await?;

    let Some((current_url, current_env, label, notes, agent_user, agent_secret, login_url, login_user, login_note, login_password)) = current else {
        return Err(ApiError::NotFound(format!("Website {id} nicht gefunden")));
    };

    // Nur wirklich geänderte Felder landen im Audit-Trail – „Adresse geändert",
    // obwohl sie gleich geblieben ist, wäre ein irreführender Eintrag.
    let mut changes: Vec<String> = Vec::new();

    if let Some(raw) = payload.url.as_deref() {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err(ApiError::BadRequest(
                "Bitte eine Internetadresse angeben.".to_string(),
            ));
        }

        let url = normalize_url(trimmed)?;
        if url != current_url {
            let taken: Option<String> =
                sqlx::query_scalar("SELECT id FROM sites WHERE url = $1 AND id != $2")
                    .bind(&url)
                    .bind(&id)
                    .fetch_optional(&state.pool)
                    .await?;

            if taken.is_some() {
                return Err(ApiError::Conflict(
                    "Diese Adresse ist bereits als Website hinterlegt.".to_string(),
                ));
            }

            sqlx::query("UPDATE sites SET url = $1 WHERE id = $2")
                .bind(&url)
                .bind(&id)
                .execute(&state.pool)
                .await?;
            changes.push("Adresse".to_string());
        }
    }

    if let Some(environment) = payload.environment.as_deref() {
        if !matches!(environment, "prod" | "staging" | "local") {
            return Err(ApiError::BadRequest(
                "Umgebung muss prod, staging oder local sein.".to_string(),
            ));
        }
        if environment != current_env {
            sqlx::query("UPDATE sites SET environment = $1 WHERE id = $2")
                .bind(environment)
                .bind(&id)
                .execute(&state.pool)
                .await?;
            changes.push("Umgebung".to_string());
        }
    }

    // Jedes dieser Felder darf geleert werden – deshalb der Umweg über die
    // einzelnen Option-Ebenen statt eines Sammel-UPDATE.
    let text_fields: [(&str, Option<Option<String>>, &str, Option<&str>); 9] = [
        ("label", payload.label, "Bezeichnung", label.as_deref()),
        ("notes", payload.notes, "Notiz", notes.as_deref()),
        (
            "agent_user",
            payload.agent_user,
            "Agent-Benutzer",
            agent_user.as_deref(),
        ),
        // Passwörter werden nie zurückgegeben, also immer als geändert führen,
        // wenn eines mitgeschickt wurde.
        ("agent_password", payload.agent_password, "Agent-Passwort", None),
        (
            "agent_secret",
            payload.agent_secret,
            "Agent-Secret",
            agent_secret.as_deref(),
        ),
        (
            "login_url",
            payload.login_url,
            "Login-Adresse",
            login_url.as_deref(),
        ),
        (
            "login_user",
            payload.login_user,
            "Login-Benutzer",
            login_user.as_deref(),
        ),
        (
            "login_password",
            payload.login_password,
            "Login-Passwort",
            login_password.as_deref(),
        ),
        (
            "login_note",
            payload.login_note,
            "Login-Notiz",
            login_note.as_deref(),
        ),
    ];

    for (column, value, label, previous) in text_fields {
        let Some(next) = value else {
            continue;
        };

        let next = next.and_then(|raw| {
            let trimmed = raw.trim().to_string();
            (!trimmed.is_empty()).then_some(trimmed)
        });

        let unchanged = match (next.as_deref(), previous) {
            (None, None) => true,
            (Some(new), Some(old)) => new == old,
            _ => false,
        };

        if unchanged {
            continue;
        }

        sqlx::query(&format!("UPDATE sites SET {column} = $1 WHERE id = $2"))
            .bind(next.as_deref())
            .bind(&id)
            .execute(&state.pool)
            .await?;

        changes.push(label.to_string());
    }

    let site = sqlx::query_as::<_, Site>("SELECT * FROM sites WHERE id = $1")
        .bind(&id)
        .fetch_one(&state.pool)
        .await?;

    audit(
        &state.pool,
        user.actor(),
        "Website geändert",
        Some("site"),
        Some(&id),
        &format!(
            "{} · geändert: {}",
            site.url,
            if changes.is_empty() {
                "nichts".to_string()
            } else {
                changes.join(", ")
            }
        ),
    )
    .await?;

    Ok(Json(site))
}

/// Entfernt eine Website samt ihrer Agent-Aktionen.
///
/// Zeiten bleiben erhalten: sie verlieren nur den Bezug zur Website, damit die
/// Abrechnung nicht rückwirkend Lücken bekommt.
pub async fn delete_site(
    State(state): State<SharedState>,
    user: CurrentUser,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    let site: Option<(String, Option<String>)> =
        sqlx::query_as("SELECT url, web_client_id FROM sites WHERE id = $1")
            .bind(&id)
            .fetch_optional(&state.pool)
            .await?;

    let Some((url, web_client_id)) = site else {
        return Err(ApiError::NotFound(format!("Website {id} nicht gefunden")));
    };

    let entries: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM time_entries WHERE site_id = $1")
        .bind(&id)
        .fetch_one(&state.pool)
        .await?;

    sqlx::query("UPDATE time_entries SET site_id = NULL WHERE site_id = $1")
        .bind(&id)
        .execute(&state.pool)
        .await?;

    sqlx::query("DELETE FROM sites WHERE id = $1")
        .bind(&id)
        .execute(&state.pool)
        .await?;

    audit(
        &state.pool,
        user.actor(),
        "Website gelöscht",
        Some("site"),
        Some(&id),
        &format!(
            "{url} · Web-Kunde {} · {entries} Zeiteinträge blieben erhalten",
            web_client_id.unwrap_or_else(|| "–".to_string())
        ),
    )
    .await?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn list_site_actions(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> ApiResult<Json<Vec<WebAction>>> {
    let actions = sqlx::query_as::<_, WebAction>(
        "SELECT * FROM web_actions WHERE site_id = $1 ORDER BY created_at DESC LIMIT 20",
    )
    .bind(&id)
    .fetch_all(&state.pool)
    .await?;

    Ok(Json(actions))
}

pub async fn get_action(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> ApiResult<Json<WebAction>> {
    let action = sqlx::query_as::<_, WebAction>("SELECT * FROM web_actions WHERE id = $1")
        .bind(&id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Aktion {id} nicht gefunden")))?;

    Ok(Json(action))
}

pub async fn run_action(
    State(state): State<SharedState>,
    user: CurrentUser,
    Path(id): Path<String>,
    Json(payload): Json<WebActionRequest>,
) -> ApiResult<Json<WebAction>> {
    if !matches!(payload.kind.as_str(), "health" | "plugins" | "core") {
        return Err(ApiError::BadRequest(
            "Aktion muss health, plugins oder core sein.".to_string(),
        ));
    }

    if state.ninja.is_mock() == false && payload.kind != "health" {
        // Backup-Pflicht gilt fuer schreibende Aktionen.
        let confirmed: Option<i64> =
            sqlx::query_scalar("SELECT backup_confirmed FROM sites WHERE id = $1")
                .bind(&id)
                .fetch_optional(&state.pool)
                .await?;

        if confirmed.unwrap_or(0) == 0 && !payload.backup_confirmed.unwrap_or(false) {
            return Err(ApiError::Conflict(
                "Backup nicht bestätigt. Vor einem Update muss ein verifiziertes Backup vorliegen."
                    .to_string(),
            ));
        }
    }

    let site: Option<(String, Option<String>, Option<String>, Option<String>, Option<String>)> =
        sqlx::query_as(
            "SELECT url, web_client_id, agent_user, agent_password, agent_secret FROM sites WHERE id = $1",
        )
        .bind(&id)
        .fetch_optional(&state.pool)
        .await?;

    let Some((url, web_client_id, agent_user, password, secret)) = site else {
        return Err(ApiError::NotFound(format!("Website {id} nicht gefunden")));
    };

    let Some(web_client_id) = web_client_id else {
        return Err(ApiError::BadRequest(
            "Diese Website hängt noch an einem NinjaOne-Kunden. Bitte im Bereich Websites neu anlegen."
                .to_string(),
        ));
    };

    let action_id = Uuid::new_v4().to_string();
    let now = now_iso();

    sqlx::query(
        "INSERT INTO web_actions
         (id, site_id, web_client_id, kind, status, progress, message, created_at)
         VALUES ($1, $2, $3, $4, 'queued', 0, 'Aktion wird vorbereitet', $5)",
    )
    .bind(&action_id)
    .bind(&id)
    .bind(&web_client_id)
    .bind(&payload.kind)
    .bind(&now)
    .execute(&state.pool)
    .await?;

    let agent = WpAgent::new(WpAgentConfig {
        site_url: url.clone(),
        user: agent_user.unwrap_or_default(),
        app_password: password.unwrap_or_default(),
        secret: secret.unwrap_or_default(),
    })
    .map_err(|error| ApiError::BadRequest(error.to_string()))?;

    let kind = payload.kind.clone();
    let actor = user.display_name.clone();
    let action_for_task = action_id.clone();
    let site_for_task = id.clone();
    let state_for_task = state.clone();
    let backup = payload.backup_confirmed.unwrap_or(false);

    tokio::spawn(async move {
        run_action_background(
            state_for_task,
            agent,
            kind,
            site_for_task,
            action_for_task,
            url,
            backup,
            actor,
        )
        .await;
    });

    let action = sqlx::query_as::<_, WebAction>("SELECT * FROM web_actions WHERE id = $1")
        .bind(&action_id)
        .fetch_one(&state.pool)
        .await?;

    Ok(Json(action))
}

#[allow(clippy::too_many_arguments)]
async fn run_action_background(
    state: SharedState,
    agent: WpAgent,
    kind: String,
    site_id: String,
    action_id: String,
    url: String,
    backup_confirmed: bool,
    actor: String,
) {
    let started = Instant::now();

    let _ = progress(&state.pool, &action_id, 25, "Agent wird angesprochen").await;

    let outcome = match kind.as_str() {
        "health" => run_health(&state, &agent, &site_id).await,
        "plugins" => run_plugins(&agent, &site_id, backup_confirmed).await,
        _ => run_core(&agent, &site_id, backup_confirmed).await,
    };

    let duration_ms = started.elapsed().as_millis() as i64;
    let now = now_iso();

    match outcome {
        Ok(result) => {
            let _ = sqlx::query(
                "UPDATE web_actions SET status = $1, progress = 100, message = $2, detail = $3,
                     before_version = $4, after_version = $5, duration_ms = $6, finished_at = $7
                 WHERE id = $8",
            )
            .bind(if result.failed { "failed" } else { "succeeded" })
            .bind(&result.summary)
            .bind(&result.detail)
            .bind(result.before.as_deref())
            .bind(result.after.as_deref())
            .bind(duration_ms)
            .bind(&now)
            .bind(&action_id)
            .execute(&state.pool)
            .await;

            let _ = sqlx::query(
                "UPDATE sites SET status = $1, last_checked = $2 WHERE id = $3",
            )
            .bind(if result.failed { "warning" } else { "healthy" })
            .bind(&now)
            .bind(&site_id)
            .execute(&state.pool)
            .await;

            let _ = audit(
                &state.pool,
                actor.as_str(),
                match kind.as_str() {
                    "health" => "Website-Health-Check",
                    "plugins" => "Website-Plugin-Update",
                    _ => "Website-Core-Update",
                },
                Some("site"),
                Some(&site_id),
                &result.summary,
            )
            .await;

            // Maschinenzeit erfassen: belegt den Aufwand, ist nicht abrechenbar.
            if duration_ms > 500 {
                let entry_id = Uuid::new_v4().to_string();
                let _ = sqlx::query(
                    "INSERT INTO time_entries
                     (id, customer_id, web_client_id, site_id, run_id, task_id, user_name,
                      activity, note, started_at, ended_at, duration_seconds, billable, source, created_at)
                     VALUES ($1, NULL,
                             (SELECT web_client_id FROM sites WHERE id = $2), $2, NULL, NULL, 'System',
                             $3, $4, $5, $6, $7, 0, 'machine', $6)",
                )
                .bind(&entry_id)
                .bind(&site_id)
                .bind(format!("NinjaOne/Agent: {} (Maschinenzeit)", action_label(&kind)))
                .bind(format!("Automatisch erfasst für {url}"))
                .bind(&now)
                .bind(&now)
                .bind((duration_ms / 1000).max(1))
                .execute(&state.pool)
                .await;
            }
        }
        Err(error) => {
            let message = error.to_string();
            let _ = sqlx::query(
                "UPDATE web_actions SET status = 'failed', message = $1, duration_ms = $2, finished_at = $3 WHERE id = $4",
            )
            .bind(&message)
            .bind(duration_ms)
            .bind(&now)
            .bind(&action_id)
            .execute(&state.pool)
            .await;

            let _ = sqlx::query("UPDATE sites SET status = 'critical', last_checked = $1 WHERE id = $2")
                .bind(&now)
                .bind(&site_id)
                .execute(&state.pool)
                .await;

            let _ = audit(
                &state.pool,
                actor.as_str(),
                "Website-Aktion fehlgeschlagen",
                Some("site"),
                Some(&site_id),
                &message,
            )
            .await;
        }
    }
}

struct ActionResult {
    summary: String,
    detail: String,
    before: Option<String>,
    after: Option<String>,
    failed: bool,
}

fn action_label(kind: &str) -> &'static str {
    match kind {
        "health" => "Health-Check",
        "plugins" => "Plugin-Update",
        _ => "Core-Update",
    }
}

async fn run_health(state: &SharedState, agent: &WpAgent, site_id: &str) -> anyhow::Result<ActionResult> {
    let data = agent.health().await?;

    let wp_version = data.core.as_ref().and_then(|core| core.version.clone());
    let php_version = data.php.as_ref().and_then(|php| php.version.clone());
    let plugin_updates = data
        .plugins
        .as_ref()
        .and_then(|plugins| plugins.updates_available)
        .unwrap_or(0);
    let core_update = data
        .core
        .as_ref()
        .and_then(|core| core.update_available)
        .unwrap_or(false);

    sqlx::query(
        "UPDATE sites SET wp_version = COALESCE($1, wp_version),
             php_version = COALESCE($2, php_version),
             plugin_updates = $3, core_update = $4, last_checked = $5
         WHERE id = $6",
    )
    .bind(wp_version.clone())
    .bind(php_version.clone())
    .bind(plugin_updates)
    .bind(core_update)
    .bind(now_iso())
    .bind(site_id)
    .execute(&state.pool)
    .await?;

    Ok(ActionResult {
        summary: format!(
            "WordPress {}, PHP {}, {} Plugin-Updates offen",
            wp_version.unwrap_or_else(|| "?".to_string()),
            php_version.unwrap_or_else(|| "?".to_string()),
            plugin_updates
        ),
        detail: serde_json::to_string(&data).unwrap_or_default(),
        before: None,
        after: None,
        failed: false,
    })
}

async fn run_plugins(
    agent: &WpAgent,
    _site_id: &str,
    backup_confirmed: bool,
) -> anyhow::Result<ActionResult> {
    let plan = agent.update_plugins(true, false).await?;
    let planned = plan.plugins.clone().unwrap_or_default();
    let actionable: Vec<_> = planned
        .iter()
        .filter(|item| !item.skipped.unwrap_or(false) && item.error.is_none())
        .collect();

    if actionable.is_empty() {
        return Ok(ActionResult {
            summary: "Keine Plugin-Updates verfügbar.".to_string(),
            detail: serde_json::to_string(&plan).unwrap_or_default(),
            before: None,
            after: None,
            failed: false,
        });
    }

    let result = agent.update_plugins(false, backup_confirmed).await?;

    let updated: Vec<String> = result
        .plugins
        .clone()
        .unwrap_or_default()
        .iter()
        .filter(|item| item.success.unwrap_or(false))
        .map(|item| {
            format!(
                "{} {} → {}",
                item.name.clone().unwrap_or_else(|| "Plugin".to_string()),
                item.version_before.clone().unwrap_or_default(),
                item.version_after.clone().unwrap_or_default()
            )
        })
        .collect();

    let failed: Vec<String> = result
        .plugins
        .clone()
        .unwrap_or_default()
        .iter()
        .filter(|item| item.success == Some(false))
        .map(|item| {
            format!(
                "{}: {}",
                item.name.clone().unwrap_or_else(|| "Plugin".to_string()),
                item.error.clone().unwrap_or_else(|| "unbekannter Fehler".to_string())
            )
        })
        .collect();

    let is_failed = !failed.is_empty();

    Ok(ActionResult {
        summary: if is_failed {
            format!("Plugin-Update fehlgeschlagen – {}", failed.join("; "))
        } else if updated.is_empty() {
            "Plugin-Updates ausgeführt, keine Versionsänderung gemeldet.".to_string()
        } else {
            format!("Plugin-Updates abgeschlossen: {}", updated.join(", "))
        },
        detail: serde_json::json!({ "dry_run": plan.plugins, "result": result.plugins })
            .to_string(),
        before: None,
        after: None,
        failed: is_failed,
    })
}

async fn run_core(
    agent: &WpAgent,
    _site_id: &str,
    backup_confirmed: bool,
) -> anyhow::Result<ActionResult> {
    let plan = agent.update_core(true, false).await?;

    let current = plan.current_version.clone();
    if plan.target_version.is_none() || current == plan.target_version {
        return Ok(ActionResult {
            summary: format!(
                "Kein Core-Update verfügbar. Installation ist auf {}.",
                current.unwrap_or_else(|| "unbekannt".to_string())
            ),
            detail: serde_json::to_string(&plan).unwrap_or_default(),
            before: None,
            after: None,
            failed: false,
        });
    }

    let result = agent.update_core(false, backup_confirmed).await?;
    let failed = result.success == Some(false);

    Ok(ActionResult {
        summary: if failed {
            format!(
                "Core-Update fehlgeschlagen: {}",
                result.error.clone().unwrap_or_else(|| "unbekannter Fehler".to_string())
            )
        } else {
            format!(
                "Core aktualisiert: {} → {}",
                result.version_before.clone().unwrap_or_else(|| "?".to_string()),
                result.version_after.clone().unwrap_or_else(|| "?".to_string())
            )
        },
        detail: serde_json::to_string(&result).unwrap_or_default(),
        before: result.version_before.clone(),
        after: result.version_after.clone(),
        failed,
    })
}

// ---------------------------------------------------------------------------
// Zeiterfassung fuer Websites
// ---------------------------------------------------------------------------

pub async fn start_timer(
    State(state): State<SharedState>,
    user: CurrentUser,
    Json(payload): Json<WebTimerRequest>,
) -> ApiResult<Json<Option<ActiveTimer>>> {
    fetch_client(&state.pool, &payload.web_client_id).await?;

    if let Some(site_id) = payload.site_id.as_deref() {
        let owner: Option<Option<String>> =
            sqlx::query_scalar("SELECT web_client_id FROM sites WHERE id = $1")
                .bind(site_id)
                .fetch_optional(&state.pool)
                .await?;

        match owner {
            None => return Err(ApiError::NotFound(format!("Website {site_id} nicht gefunden"))),
            Some(Some(owner_id)) if owner_id != payload.web_client_id => {
                return Err(ApiError::BadRequest(
                    "Die Website gehört nicht zu diesem Web-Kunden.".to_string(),
                ))
            }
            _ => {}
        }
    }

    // Einen laufenden Timer dieser Person sauber beenden.
    if let Some(active) = queries::active_timer(&state.pool, &user.id).await? {
        let started = parse_timestamp(&active.started_at).unwrap_or_else(chrono::Utc::now);
        let seconds = (chrono::Utc::now() - started).num_seconds().max(1);
        sqlx::query("UPDATE time_entries SET ended_at = $1, duration_seconds = $2 WHERE id = $3")
            .bind(now_iso())
            .bind(seconds)
            .bind(&active.id)
            .execute(&state.pool)
            .await?;
    }

    let id = Uuid::new_v4().to_string();
    let now = now_iso();

    sqlx::query(
        "INSERT INTO time_entries
         (id, customer_id, web_client_id, site_id, run_id, task_id, user_id, user_name,
          activity, note, started_at, ended_at, duration_seconds, billable, source, created_at)
         VALUES ($1, NULL, $2, $3, NULL, NULL, $8, $7, $4, $5, $6, NULL, 0, true, 'timer', $6)",
    )
    .bind(&id)
    .bind(&payload.web_client_id)
    .bind(payload.site_id.as_deref())
    .bind(
        payload
            .activity
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("Website-Betreuung"),
    )
    .bind(payload.comment.as_deref())
    .bind(&now)
    .bind(user.actor())
    .bind(&user.id)
    .execute(&state.pool)
    .await?;

    audit(
        &state.pool,
        user.actor(),
        "Timer gestartet",
        Some("web_client"),
        Some(&payload.web_client_id),
        payload.comment.as_deref().unwrap_or("ohne Kommentar"),
    )
    .await?;

    Ok(Json(queries::active_timer(&state.pool, &user.id).await?))
}

pub async fn create_manual_entry(
    State(state): State<SharedState>,
    user: CurrentUser,
    Json(payload): Json<WebManualTimeRequest>,
) -> ApiResult<Json<Option<TimeEntryListItem>>> {
    if payload.minutes < 1 || payload.minutes > 24 * 60 {
        return Err(ApiError::BadRequest(
            "Die Dauer muss zwischen 1 und 1440 Minuten liegen.".to_string(),
        ));
    }

    if payload.activity.trim().is_empty() {
        return Err(ApiError::BadRequest(
            "Bitte eine Leistungsart angeben.".to_string(),
        ));
    }

    fetch_client(&state.pool, &payload.web_client_id).await?;

    let (started_at, ended_at) = match payload.date.as_deref() {
        Some(value) => {
            let date = chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d").map_err(|_| {
                ApiError::BadRequest("Datum muss im Format YYYY-MM-DD vorliegen.".to_string())
            })?;
            let start = date
                .and_hms_opt(9, 0, 0)
                .ok_or_else(|| ApiError::BadRequest("Ungültige Uhrzeit.".to_string()))?;
            (
                start.format("%Y-%m-%dT%H:%M:%S").to_string(),
                (start + chrono::Duration::minutes(payload.minutes))
                    .format("%Y-%m-%dT%H:%M:%S")
                    .to_string(),
            )
        }
        None => {
            let start = chrono::Utc::now();
            (
                start.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
                (start + chrono::Duration::minutes(payload.minutes))
                    .to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            )
        }
    };

    let id = Uuid::new_v4().to_string();

    sqlx::query(
        "INSERT INTO time_entries
         (id, customer_id, web_client_id, site_id, run_id, task_id, user_name,
          activity, note, started_at, ended_at, duration_seconds, billable, source, created_at)
         VALUES ($1, NULL, $2, $3, NULL, NULL, $11, $4, $5, $6, $7, $8, $9, 'manual', $10)",
    )
    .bind(&id)
    .bind(&payload.web_client_id)
    .bind(payload.site_id.as_deref())
    .bind(payload.activity.trim())
    .bind(payload.comment.as_deref())
    .bind(&started_at)
    .bind(&ended_at)
    .bind(payload.minutes * 60)
    .bind(payload.billable)
    .bind(now_iso())
    .bind(user.actor())
    .execute(&state.pool)
    .await?;

    audit(
        &state.pool,
        user.actor(),
        "Zeit manuell erfasst",
        Some("web_client"),
        Some(&payload.web_client_id),
        &format!("{} · {} Minuten", payload.activity, payload.minutes),
    )
    .await?;

    Ok(Json(queries::time_entry(&state.pool, &id).await?))
}

pub async fn update_comment(
    State(state): State<SharedState>,
    _user: CurrentUser,
    Path(id): Path<String>,
    Json(payload): Json<UpdateCommentRequest>,
) -> ApiResult<Json<Option<TimeEntryListItem>>> {
    let exists: Option<String> = sqlx::query_scalar("SELECT id FROM time_entries WHERE id = $1")
        .bind(&id)
        .fetch_optional(&state.pool)
        .await?;

    if exists.is_none() {
        return Err(ApiError::NotFound(format!("Zeiteintrag {id} nicht gefunden")));
    }

    let comment = payload
        .comment
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());

    sqlx::query("UPDATE time_entries SET note = $1 WHERE id = $2")
        .bind(comment)
        .bind(&id)
        .execute(&state.pool)
        .await?;

    Ok(Json(queries::time_entry(&state.pool, &id).await?))
}

// ---------------------------------------------------------------------------
// Hilfen
// ---------------------------------------------------------------------------

async fn progress(
    pool: &PgPool,
    action_id: &str,
    percent: i64,
    message: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE web_actions SET status = 'running', progress = $1, message = $2 WHERE id = $3",
    )
    .bind(percent)
    .bind(message)
    .bind(action_id)
    .execute(pool)
    .await?;

    Ok(())
}

async fn fetch_client(pool: &PgPool, id: &str) -> ApiResult<WebClient> {
    sqlx::query_as::<_, WebClient>("SELECT * FROM web_clients WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Web-Kunde {id} nicht gefunden")))
}

/// Ergänzt fehlendes Schema und normalisiert die Adresse.
fn normalize_url(raw: &str) -> ApiResult<String> {
    let trimmed = raw.trim();
    let with_scheme = if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        trimmed.to_string()
    } else {
        format!("https://{trimmed}")
    };

    let parsed = reqwest::Url::parse(&with_scheme)
        .map_err(|_| ApiError::BadRequest("Die Adresse ist nicht gültig.".to_string()))?;

    if parsed.host_str().is_none() {
        return Err(ApiError::BadRequest(
            "Die Adresse enthält keinen Hostnamen.".to_string(),
        ));
    }

    Ok(with_scheme.trim_end_matches('/').to_string())
}

fn derive_short_code(name: &str) -> String {
    let code: String = name
        .split(|c: char| c.is_whitespace() || c == '-' || c == '&' || c == '.')
        .filter(|word| !word.is_empty())
        .take(2)
        .map(|word| word.chars().next().unwrap_or('W').to_uppercase().to_string())
        .collect();

    if code.len() < 2 {
        name.chars()
            .filter(|c| c.is_alphanumeric())
            .take(2)
            .collect::<String>()
            .to_uppercase()
    } else {
        code
    }
}
