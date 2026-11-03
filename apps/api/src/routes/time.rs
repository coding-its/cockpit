use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use chrono::{Duration, NaiveDate, SecondsFormat, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::auth::CurrentUser;
use crate::billing;
use crate::error::{ApiError, ApiResult};
use crate::export;
use crate::models::{
    ActiveTimer, BillingFilter, BillingSummary, DeviceTimerRequest, ExportFilter, ManualTimeRequest,
    Task, TimeEntryFilter, TimeEntryListItem, TimerStartRequest, TimerStopResponse,
};
use crate::queries;
use crate::state::SharedState;
use crate::util::{audit, current_month, now_iso, parse_timestamp};

pub async fn active(
    State(state): State<SharedState>,
    user: CurrentUser,
) -> ApiResult<Json<Option<ActiveTimer>>> {
    Ok(Json(queries::active_timer(&state.pool, &user.id).await?))
}

/// Alle laufenden Timer im Team – „wer arbeitet gerade woran".
pub async fn running(
    State(state): State<SharedState>,
) -> ApiResult<Json<Vec<ActiveTimer>>> {
    Ok(Json(queries::running_timers(&state.pool).await?))
}

/// Timer direkt auf einem Gerät starten. Der Kunde wird vom Gerät abgeleitet.
pub async fn start_device(
    State(state): State<SharedState>,
    user: CurrentUser,
    Json(payload): Json<DeviceTimerRequest>,
) -> ApiResult<Json<Option<ActiveTimer>>> {
    let device: Option<(String, String, Option<String>)> = sqlx::query_as(
        "SELECT hostname, customer_id, os FROM devices WHERE id = $1",
    )
    .bind(&payload.device_id)
    .fetch_optional(&state.pool)
    .await?;

    let Some((hostname, customer_id, _os)) = device else {
        return Err(ApiError::NotFound(format!(
            "Gerät {} nicht gefunden",
            payload.device_id
        )));
    };

    if let Some(stopped) = stop_active(&state.pool, &user.id).await? {
        audit(
            &state.pool,
            user.actor(),
            "Timer automatisch gestoppt",
            Some("time_entry"),
            None,
            &format!(
                "{} · {} Minuten",
                stopped.activity,
                stopped.duration_seconds / 60
            ),
        )
        .await?;
    }

    let id = Uuid::new_v4().to_string();
    let now = now_iso();

    let activity = payload
        .activity
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("Gerätewartung");

    sqlx::query(
        "INSERT INTO time_entries
         (id, customer_id, web_client_id, site_id, device_id, run_id, task_id, user_id, user_name,
          activity, note, started_at, ended_at, duration_seconds, billable, source, created_at)
         VALUES ($1, $2, NULL, NULL, $3, NULL, NULL, $8, $4, $5, $6, $7, NULL, 0, true, 'timer', $7)",
    )
    .bind(&id)
    .bind(&customer_id)
    .bind(&payload.device_id)
    .bind(user.actor())
    .bind(activity)
    .bind(payload.comment.as_deref())
    .bind(&now)
    .bind(&user.id)
    .execute(&state.pool)
    .await?;

    audit(
        &state.pool,
        user.actor(),
        "Timer gestartet",
        Some("device"),
        Some(&payload.device_id),
        &format!(
            "{hostname} · {activity}{}",
            payload
                .comment
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .map(|value| format!(" · {value}"))
                .unwrap_or_default()
        ),
    )
    .await?;

    Ok(Json(queries::active_timer(&state.pool, &user.id).await?))
}

pub async fn start(
    State(state): State<SharedState>,
    user: CurrentUser,
    Json(payload): Json<TimerStartRequest>,
) -> ApiResult<Json<Option<ActiveTimer>>> {
    let task = sqlx::query_as::<_, Task>("SELECT * FROM tasks WHERE id = $1")
        .bind(&payload.task_id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| {
            ApiError::NotFound(format!("Aufgabe {} nicht gefunden", payload.task_id))
        })?;

    let customer_id: String = sqlx::query_scalar("SELECT customer_id FROM runs WHERE id = $1")
        .bind(&task.run_id)
        .fetch_one(&state.pool)
        .await?;

    if let Some(stopped) = stop_active(&state.pool, &user.id).await? {
        audit(
            &state.pool,
            user.actor(),
            "Timer automatisch gestoppt",
            Some("time_entry"),
            None,
            &format!(
                "{} · {} Minuten",
                stopped.activity,
                stopped.duration_seconds / 60
            ),
        )
        .await?;
    }

    let id = Uuid::new_v4().to_string();
    let now = now_iso();

    sqlx::query(
        "INSERT INTO time_entries
         (id, customer_id, run_id, task_id, user_id, user_name, activity, note, started_at, ended_at, duration_seconds, billable, source, created_at)
         VALUES ($1, $2, $3, $4, $9, $8, $5, $6, $7, NULL, 0, true, 'timer', $7)",
    )
    .bind(&id)
    .bind(&customer_id)
    .bind(&task.run_id)
    .bind(&task.id)
    .bind(&task.title)
    .bind(payload.note.as_deref())
    .bind(&now)
    .bind(user.actor())
    .bind(&user.id)
    .execute(&state.pool)
    .await?;

    audit(
        &state.pool,
        user.actor(),
        "Timer gestartet",
        Some("task"),
        Some(&task.id),
        &format!("Timer gestartet für: {}", task.title),
    )
    .await?;

    Ok(Json(queries::active_timer(&state.pool, &user.id).await?))
}

pub async fn stop(
    State(state): State<SharedState>,
    user: CurrentUser,
) -> ApiResult<Json<Option<TimerStopResponse>>> {
    let stopped = stop_active(&state.pool, &user.id).await?;

    if let Some(response) = &stopped {
        audit(
            &state.pool,
            user.actor(),
            "Timer gestoppt",
            Some("time_entry"),
            None,
            &format!(
                "{} · {} Minuten",
                response.activity,
                response.duration_seconds / 60
            ),
        )
        .await?;
    }

    Ok(Json(stopped))
}

/// Beendet den laufenden Timer **dieser Person**. Ein fremder Timer bleibt
/// unangetastet – früher beendete jeder Start den neuesten Eintrag im System.
async fn stop_active(pool: &PgPool, user_id: &str) -> ApiResult<Option<TimerStopResponse>> {
    let Some(timer) = queries::active_timer(pool, user_id).await? else {
        return Ok(None);
    };

    let started = parse_timestamp(&timer.started_at).unwrap_or_else(Utc::now);
    let duration_seconds = (Utc::now() - started).num_seconds().max(1);
    let ended_at = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);

    sqlx::query("UPDATE time_entries SET ended_at = $1, duration_seconds = $2 WHERE id = $3")
        .bind(&ended_at)
        .bind(duration_seconds)
        .bind(&timer.id)
        .execute(pool)
        .await?;

    Ok(Some(TimerStopResponse {
        stopped: true,
        duration_seconds,
        id: timer.id,
        activity: timer.activity,
        customer_name: timer.customer_name,
        customer_short_code: timer.customer_short_code,
    }))
}

pub async fn list(
    State(state): State<SharedState>,
    Query(filter): Query<TimeEntryFilter>,
) -> ApiResult<Json<Vec<TimeEntryListItem>>> {
    let limit = filter.limit.unwrap_or(250).clamp(1, 1000);
    let entries = queries::time_entries(
        &state.pool,
        filter.customer_id.as_deref(),
        filter.web_client_id.as_deref(),
        filter.month.as_deref(),
        limit,
    )
    .await?;

    Ok(Json(entries))
}

pub async fn create(
    State(state): State<SharedState>,
    user: CurrentUser,
    Json(payload): Json<ManualTimeRequest>,
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

    let customer_exists: Option<String> =
        sqlx::query_scalar("SELECT id FROM customers WHERE id = $1")
            .bind(&payload.customer_id)
            .fetch_optional(&state.pool)
            .await?;

    if customer_exists.is_none() {
        return Err(ApiError::NotFound(format!(
            "Kunde {} nicht gefunden",
            payload.customer_id
        )));
    }

    let (started_at, ended_at) = match payload.date.as_deref() {
        Some(value) => {
            let date = NaiveDate::parse_from_str(value, "%Y-%m-%d").map_err(|_| {
                ApiError::BadRequest("Datum muss im Format YYYY-MM-DD vorliegen.".to_string())
            })?;
            let start = date
                .and_hms_opt(9, 0, 0)
                .ok_or_else(|| ApiError::BadRequest("Ungültige Uhrzeit.".to_string()))?;
            (
                start.format("%Y-%m-%dT%H:%M:%S").to_string(),
                (start + Duration::minutes(payload.minutes))
                    .format("%Y-%m-%dT%H:%M:%S")
                    .to_string(),
            )
        }
        None => {
            let start = Utc::now();
            (
                start.to_rfc3339_opts(SecondsFormat::Secs, true),
                (start + Duration::minutes(payload.minutes))
                    .to_rfc3339_opts(SecondsFormat::Secs, true),
            )
        }
    };

    let (task_id, run_id) = match payload.task_id.as_deref() {
        Some(task_id) => {
            let run_id: Option<String> =
                sqlx::query_scalar("SELECT run_id FROM tasks WHERE id = $1")
                    .bind(task_id)
                    .fetch_optional(&state.pool)
                    .await?;
            let run_id = run_id.ok_or_else(|| {
                ApiError::NotFound(format!("Aufgabe {task_id} nicht gefunden"))
            })?;
            (Some(task_id.to_string()), Some(run_id))
        }
        None => (None, None),
    };

    let id = Uuid::new_v4().to_string();

    sqlx::query(
        "INSERT INTO time_entries
         (id, customer_id, run_id, task_id, user_id, user_name, activity, note, started_at, ended_at, duration_seconds, billable, source, created_at)
         VALUES ($1, $2, $3, $4, $13, $12, $5, $6, $7, $8, $9, $10, 'manual', $11)",
    )
    .bind(&id)
    .bind(&payload.customer_id)
    .bind(&run_id)
    .bind(&task_id)
    .bind(payload.activity.trim())
    .bind(payload.note.as_deref())
    .bind(&started_at)
    .bind(&ended_at)
    .bind(payload.minutes * 60)
    .bind(payload.billable)
    .bind(now_iso())
    .bind(user.actor())
    .bind(&user.id)
    .execute(&state.pool)
    .await?;

    audit(
        &state.pool,
        user.actor(),
        "Zeit manuell erfasst",
        Some("time_entry"),
        Some(&id),
        &format!("{} · {} Minuten", payload.activity, payload.minutes),
    )
    .await?;

    Ok(Json(queries::time_entry(&state.pool, &id).await?))
}

pub async fn remove(
    State(state): State<SharedState>,
    user: CurrentUser,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    let result = sqlx::query("DELETE FROM time_entries WHERE id = $1")
        .bind(&id)
        .execute(&state.pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(ApiError::NotFound(format!("Zeiteintrag {id} nicht gefunden")));
    }

    audit(
        &state.pool,
        user.actor(),
        "Zeiteintrag gelöscht",
        Some("time_entry"),
        Some(&id),
        "Eintrag entfernt.",
    )
    .await?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn billing_summary(
    State(state): State<SharedState>,
    Query(filter): Query<BillingFilter>,
) -> ApiResult<Json<BillingSummary>> {
    let month = filter.month.unwrap_or_else(current_month);
    let rate = crate::settings::default_hourly_rate(&state.pool).await;
    Ok(Json(billing::summary(&state.pool, &month, false, rate).await?))
}

pub async fn export_csv(
    State(state): State<SharedState>,
    user: CurrentUser,
    Query(filter): Query<ExportFilter>,
) -> ApiResult<Response> {
    let month = filter.month.unwrap_or_else(current_month);
    let entries = queries::time_entries(
        &state.pool,
        filter.customer_id.as_deref(),
        filter.web_client_id.as_deref(),
        Some(&month),
        10_000,
    )
    .await?;

    let entries: Vec<TimeEntryListItem> = if filter.billable_only.unwrap_or(false) {
        entries.into_iter().filter(|entry| entry.billable).collect()
    } else {
        entries
    };

    let csv = export::to_csv(&entries).map_err(ApiError::Other)?;

    audit(
        &state.pool,
        user.actor(),
        "Zeitnachweis exportiert",
        Some("export"),
        None,
        &format!("{} Einträge für {} exportiert.", entries.len(), month),
    )
    .await?;

    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/csv; charset=utf-8"),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!(
            "attachment; filename=\"{}\"",
            export::filename(&month)
        ))
        .unwrap_or_else(|_| HeaderValue::from_static("attachment; filename=\"zeiten.csv\"")),
    );

    Ok((headers, csv).into_response())
}
