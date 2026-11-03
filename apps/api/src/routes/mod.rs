pub mod auth;
pub mod time;
pub mod web;

use axum::extract::{Path, Query, State};
use axum::response::{Html, IntoResponse};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::auth::CurrentUser;
use crate::billing;
use crate::error::{ApiError, ApiResult};
use crate::executors;
use crate::models::*;
use crate::queries;
use crate::state::SharedState;
use crate::util::{audit, current_month, month_label, now_iso};

pub fn router(state: SharedState) -> Router {
    Router::new()
        .route("/api/v1/health", get(health))
        .route("/api/v1/dashboard", get(dashboard))
        .route("/api/v1/customers", get(list_customers))
        .route("/api/v1/customers/{id}", get(get_customer))
        .route("/api/v1/customers/{id}/focus", axum::routing::patch(set_customer_focus))
        .route("/api/v1/sites", post(create_site))
        .route("/api/v1/sites/{id}/agent", get(site_agent_status).patch(set_site_agent))
        .route("/api/v1/runs", get(list_runs).post(create_run))
        .route("/api/v1/tickets", get(list_tickets).post(create_ticket))
        .route("/api/v1/runs/{id}", get(get_run))
        .route("/api/v1/tasks/{id}", get(get_task))
        .route("/api/v1/tasks/{id}/approve", post(approve_task))
        .route("/api/v1/tasks/{id}/execute", post(execute_task))
        .route("/api/v1/jobs/{id}", get(get_job))
        .route("/api/v1/timer/active", get(time::active))
        .route("/api/v1/timer/running", get(time::running))
        .route("/api/v1/timer/start", post(time::start))
        .route("/api/v1/device-timer/start", post(time::start_device))
        .route(
            "/api/v1/devices/{id}/quick-action",
            post(quick_device_action),
        )
        .route(
            "/api/v1/devices/{id}/teamviewer",
            axum::routing::patch(set_device_teamviewer),
        )
        .route("/api/v1/devices/{id}/patches", get(device_patches))
        .route("/api/v1/timer/stop", post(time::stop))
        .route(
            "/api/v1/time-entries",
            get(time::list).post(time::create),
        )
        .route(
            "/api/v1/time-entries/{id}",
            axum::routing::delete(time::remove),
        )
        .route("/api/v1/billing/summary", get(time::billing_summary))
        .route("/api/v1/reports/monthly", get(monthly_report))
        .route("/api/v1/export/time-entries.csv", get(time::export_csv))
        .route("/api/v1/sync/ninjaone", post(sync_ninjaone))
        .route("/api/v1/sync/monitoring", post(sync_monitoring))
        .route(
            "/api/v1/settings/billing",
            get(get_billing_settings).put(update_billing_settings),
        )
        .route("/api/v1/settings/database", get(get_database_settings).put(update_database_settings))
        .route("/api/v1/settings/database/test", post(test_database_settings))
        .route("/api/v1/settings/env", get(get_env_settings).put(update_env_settings))
        .route("/api/v1/settings/env/test", post(test_env_settings))
        .route("/api/v1/auth/ninjaone/start", get(ninjaone_auth_start))
        .route("/api/v1/auth/ninjaone/callback", get(ninjaone_auth_callback))
        .route("/api/v1/integrations", get(integrations))
        .route("/api/v1/audit", get(list_audit))
        .merge(web::router(state.clone()))
        .merge(auth::router())
        .with_state(state)
}

pub async fn health(State(state): State<SharedState>) -> ApiResult<Json<HealthResponse>> {
    sqlx::query_scalar::<_, i64>("SELECT 1::bigint")
        .fetch_one(&state.pool)
        .await?;

    Ok(Json(HealthResponse {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
        database: "ok",
        ninjaone_mode: if state.ninja.is_mock() { "mock" } else { "live" },
    }))
}

pub async fn dashboard(
    State(state): State<SharedState>,
    user: CurrentUser,
) -> ApiResult<Json<Dashboard>> {
    let month = current_month();

    let open_runs: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM runs r JOIN customers c ON c.id = r.customer_id
         WHERE r.status IN ('planned', 'in_progress', 'blocked') AND c.active = true
           AND r.kind = 'maintenance'",
    )
    .fetch_one(&state.pool)
    .await?;

    let open_tickets: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM runs r JOIN customers c ON c.id = r.customer_id
         WHERE r.status IN ('planned', 'in_progress', 'blocked') AND c.active = true
           AND r.kind = 'ticket'",
    )
    .fetch_one(&state.pool)
    .await?;

    let customer_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM customers WHERE active = true AND focus = true")
            .fetch_one(&state.pool)
            .await?;

    let hidden_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM customers WHERE active = true AND focus = false")
            .fetch_one(&state.pool)
            .await?;

    let site_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sites s JOIN customers c ON c.id = s.customer_id WHERE c.active = true",
    )
    .fetch_one(&state.pool)
    .await?;

    let critical_sites: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sites s JOIN customers c ON c.id = s.customer_id
         WHERE s.status IN ('warning', 'critical') AND c.active = true AND c.focus = true",
    )
    .fetch_one(&state.pool)
    .await?;

    let pending_patches: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM devices d JOIN customers c ON c.id = d.customer_id
         WHERE d.patch_status = 'pending' AND c.active = true AND c.focus = true",
    )
    .fetch_one(&state.pool)
    .await?;

    let default_rate = crate::settings::default_hourly_rate(&state.pool).await;
    let billing_summary = billing::summary(&state.pool, &month, true, default_rate).await?;

    let next_runs = sqlx::query_as::<_, RunListItem>(
        r#"
        SELECT
            r.id, r.customer_id, c.name AS customer_name, c.short_code AS customer_short_code,
            r.period, r.title, r.scheduled_for, r.status, r.assignee,
            (SELECT COUNT(*) FROM tasks t WHERE t.run_id = r.id) AS total_tasks,
            (SELECT COUNT(*) FROM tasks t WHERE t.run_id = r.id AND t.status = 'done') AS done_tasks,
            r.kind, r.priority, r.description, r.due_at, r.channel
        FROM runs r
        JOIN customers c ON c.id = r.customer_id
        WHERE r.status IN ('planned', 'in_progress', 'blocked') AND c.active = true
          AND r.kind = 'maintenance' AND c.focus = true
        ORDER BY r.scheduled_for ASC
        LIMIT 5
        "#,
    )
    .fetch_all(&state.pool)
    .await?;

    let attention = build_attention(&state.pool).await?;

    let recent_activity = sqlx::query_as::<_, AuditEvent>(
        "SELECT * FROM audit_events ORDER BY created_at DESC LIMIT 8",
    )
    .fetch_all(&state.pool)
    .await?;

    let active_timer = queries::active_timer(&state.pool, &user.id).await?;
    let integration = integration_status(&state).await?;

    Ok(Json(Dashboard {
        month,
        open_runs,
        open_tickets,
        hidden_customers: hidden_count,
        billable_hours: billing_summary.total_billable_hours,
        billable_amount: billing_summary.total_amount,
        customer_count,
        site_count,
        critical_sites,
        pending_patches,
        next_runs,
        attention,
        recent_activity,
        active_timer,
        integration,
    }))
}

#[derive(Debug, FromRow)]
struct SiteAttention {
    customer_id: String,
    customer_name: String,
    label: Option<String>,
    url: String,
    status: String,
    plugin_updates: i64,
    core_update: bool,
    backup_age_hours: Option<i64>,
}

#[derive(Debug, FromRow)]
struct DeviceAttention {
    customer_id: String,
    customer_name: String,
    hostname: String,
    os: Option<String>,
    patch_pending: Option<i64>,
    antivirus_status: String,
    antivirus_product: Option<String>,
    antivirus_definitions: Option<String>,
}

async fn build_attention(pool: &PgPool) -> ApiResult<Vec<AttentionItem>> {
    let mut items = Vec::new();

    let sites = sqlx::query_as::<_, SiteAttention>(
        r#"
        SELECT s.customer_id, c.name AS customer_name, s.label, s.url, s.status,
               s.plugin_updates, s.core_update, s.backup_age_hours
        FROM sites s
        JOIN customers c ON c.id = s.customer_id
        WHERE s.status IN ('warning', 'critical') AND c.active = true AND c.focus = true
        ORDER BY CASE s.status WHEN 'critical' THEN 0 ELSE 1 END, c.name
        "#,
    )
    .fetch_all(pool)
    .await?;

    for site in sites {
        let backup = match site.backup_age_hours {
            Some(hours) => format!("Backup {hours} h alt"),
            None => "Backup unbekannt".to_string(),
        };
        let core = if site.core_update {
            "Core-Update offen"
        } else {
            "Core aktuell"
        };

        items.push(AttentionItem {
            customer_id: site.customer_id,
            customer_name: site.customer_name,
            kind: "site".to_string(),
            label: site.label.unwrap_or(site.url),
            detail: format!("{} Plugin-Updates · {} · {}", site.plugin_updates, core, backup),
            severity: site.status,
        });
    }

    let devices = sqlx::query_as::<_, DeviceAttention>(
        r#"
        SELECT d.customer_id, c.name AS customer_name, d.hostname, d.os,
               d.patch_pending, d.antivirus_status, d.antivirus_product, d.antivirus_definitions
        FROM devices d
        JOIN customers c ON c.id = d.customer_id
        WHERE (d.patch_status = 'pending' OR d.antivirus_status = 'warning')
          AND c.active = true AND c.focus = true
        ORDER BY CASE WHEN d.antivirus_status = 'warning' THEN 0 ELSE 1 END,
                 d.patch_pending DESC, c.name, d.hostname
        "#,
    )
    .fetch_all(pool)
    .await?;

    for device in devices {
        // Antivirus wiegt schwerer als ein offener Patch – ein stillgelegter
        // Schutz ist der Notfall, fehlende Updates sind Routine.
        let av_problem = device.antivirus_status == "warning";
        let detail = if av_problem {
            let product = device
                .antivirus_product
                .as_deref()
                .unwrap_or("Antivirus");
            let stale = device
                .antivirus_definitions
                .as_deref()
                .map(|value| value.eq_ignore_ascii_case("Out-of-Date"))
                .unwrap_or(false);
            format!(
                "{} · {product} {}",
                device
                    .os
                    .clone()
                    .unwrap_or_else(|| "Unbekanntes System".to_string()),
                if stale { "abgeschaltet, Signaturen veraltet" } else { "nicht aktiv" }
            )
        } else {
            format!(
                "{} · {} Patches offen",
                device
                    .os
                    .clone()
                    .unwrap_or_else(|| "Unbekanntes System".to_string()),
                device.patch_pending.unwrap_or(0)
            )
        };

        items.push(AttentionItem {
            customer_id: device.customer_id,
            customer_name: device.customer_name,
            kind: "device".to_string(),
            label: device.hostname,
            detail,
            severity: "warning".to_string(),
        });
    }

    items.sort_by_key(|item| if item.severity == "critical" { 0 } else { 1 });
    items.truncate(6);
    Ok(items)
}

pub async fn list_customers(
    State(state): State<SharedState>,
) -> ApiResult<Json<Vec<CustomerSummary>>> {
    let month = current_month();

    let customers = sqlx::query_as::<_, CustomerSummary>(
        r#"
        SELECT
            c.id, c.name, c.short_code, c.ninja_org_id, c.contact_name, c.contact_email, c.phone,
            c.focus,
            (SELECT COUNT(*) FROM devices d WHERE d.customer_id = c.id) AS device_count,
            (SELECT COUNT(*) FROM sites s WHERE s.customer_id = c.id) AS site_count,
            (SELECT COUNT(*) FROM runs r WHERE r.customer_id = c.id AND r.status IN ('planned', 'in_progress', 'blocked')) AS open_runs,
            COALESCE((
                SELECT COALESCE(SUM(te.duration_seconds), 0)::double precision / 3600.0 FROM time_entries te
                WHERE te.customer_id = c.id AND te.billable = true
                  AND substr(te.started_at, 1, 7) = $1
            ), 0.0) AS month_hours,
            (SELECT COUNT(*) FROM sites s WHERE s.customer_id = c.id AND s.status IN ('warning', 'critical'))
              + (SELECT COUNT(*) FROM devices d WHERE d.customer_id = c.id
                   AND (d.patch_status = 'pending' OR d.antivirus_status = 'warning')) AS issues
        FROM customers c
        WHERE c.active = true
        ORDER BY c.name
        "#,
    )
    .bind(month)
    .fetch_all(&state.pool)
    .await?;

    Ok(Json(customers))
}

pub async fn get_customer(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> ApiResult<Json<CustomerDetail>> {
    let customer = sqlx::query_as::<_, Customer>(
        "SELECT id, name, short_code, ninja_org_id, contact_name, contact_email, phone, notes, active, focus
         FROM customers WHERE id = $1",
    )
    .bind(&id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| ApiError::NotFound(format!("Kunde {id} nicht gefunden")))?;

    let devices = sqlx::query_as::<_, Device>(
        "SELECT * FROM devices WHERE customer_id = $1 ORDER BY device_type DESC, hostname",
    )
    .bind(&id)
    .fetch_all(&state.pool)
    .await?;

    let sites = sqlx::query_as::<_, Site>(
        "SELECT * FROM sites WHERE customer_id = $1 ORDER BY environment, label",
    )
    .bind(&id)
    .fetch_all(&state.pool)
    .await?;

    let contracts = sqlx::query_as::<_, Contract>(
        "SELECT * FROM contracts WHERE customer_id = $1 AND active = true ORDER BY name",
    )
    .bind(&id)
    .fetch_all(&state.pool)
    .await?;

    let runs = sqlx::query_as::<_, RunListItem>(
        r#"
        SELECT
            r.id, r.customer_id, c.name AS customer_name, c.short_code AS customer_short_code,
            r.period, r.title, r.scheduled_for, r.status, r.assignee,
            (SELECT COUNT(*) FROM tasks t WHERE t.run_id = r.id) AS total_tasks,
            (SELECT COUNT(*) FROM tasks t WHERE t.run_id = r.id AND t.status = 'done') AS done_tasks,
            r.kind, r.priority, r.description, r.due_at, r.channel
        FROM runs r
        JOIN customers c ON c.id = r.customer_id
        WHERE r.customer_id = $1
        ORDER BY r.scheduled_for DESC
        LIMIT 10
        "#,
    )
    .bind(&id)
    .fetch_all(&state.pool)
    .await?;

    let recent_time = queries::time_entries(&state.pool, Some(&id), None, None, 10).await?;

    Ok(Json(CustomerDetail {
        customer,
        devices,
        sites,
        contracts,
        runs,
        recent_time,
    }))
}

pub async fn create_site(
    State(state): State<SharedState>,
    _user: CurrentUser,
    Json(payload): Json<CreateSiteRequest>,
) -> ApiResult<Json<SiteAgentStatus>> {
    let raw_url = payload.url.trim();
    if raw_url.is_empty() {
        return Err(ApiError::BadRequest("Bitte eine URL angeben.".to_string()));
    }

    if !raw_url.starts_with("http://") && !raw_url.starts_with("https://") {
        return Err(ApiError::BadRequest(
            "Die URL muss mit http:// oder https:// beginnen.".to_string(),
        ));
    }

    let customer: Option<(String, i64)> =
        sqlx::query_as("SELECT name, active FROM customers WHERE id = $1")
            .bind(&payload.customer_id)
            .fetch_optional(&state.pool)
            .await?;

    let Some((customer_name, active)) = customer else {
        return Err(ApiError::NotFound(format!(
            "Kunde {} nicht gefunden",
            payload.customer_id
        )));
    };

    if active == 0 {
        return Err(ApiError::BadRequest(
            "Für diesen Kunden ist keine Betreuung aktiv.".to_string(),
        ));
    }

    // Normalisieren: ohne abschließenden Slash, damit Pfade konsistent sind.
    let url = raw_url.trim_end_matches('/').to_string();

    let exists: Option<String> = sqlx::query_scalar("SELECT id FROM sites WHERE url = $1")
        .bind(&url)
        .fetch_optional(&state.pool)
        .await?;

    if exists.is_some() {
        return Err(ApiError::Conflict(
            "Diese URL ist bereits als Site hinterlegt.".to_string(),
        ));
    }

    let environment = payload.environment.as_deref().unwrap_or("prod");
    if !matches!(environment, "prod" | "staging" | "local") {
        return Err(ApiError::BadRequest(
            "Umgebung muss prod, staging oder local sein.".to_string(),
        ));
    }

    let id = Uuid::new_v4().to_string();

    sqlx::query(
        "INSERT INTO sites
         (id, customer_id, url, label, environment, agent_mode, wp_version, php_version,
          plugin_updates, core_update, status, last_checked, backup_age_hours, notes,
          agent_user, agent_password, agent_secret, backup_confirmed)
         VALUES ($1, $2, $3, $4, $5, 'rest', NULL, NULL, 0, false, 'unknown', NULL, NULL, $6,
                 $7, $8, $9, false)",
    )
    .bind(&id)
    .bind(&payload.customer_id)
    .bind(&url)
    .bind(payload.label.as_deref().filter(|value| !value.trim().is_empty()))
    .bind(environment)
    .bind(payload.notes.as_deref())
    .bind(payload.agent_user.as_deref())
    .bind(payload.agent_password.as_deref())
    .bind(payload.agent_secret.as_deref())
    .execute(&state.pool)
    .await?;

    audit(
        &state.pool,
        _user.actor(),
        "WordPress-Site angelegt",
        Some("site"),
        Some(&id),
        &format!("{url} · {customer_name} · {environment}"),
    )
    .await?;

    site_agent_status(State(state), Path(id)).await
}

pub async fn site_agent_status(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> ApiResult<Json<SiteAgentStatus>> {
    let row: Option<(String, String, Option<String>, Option<String>, Option<String>, Option<String>, bool, Option<String>, Option<String>, Option<String>, Option<String>, i64, bool, String, Option<String>)> =
        sqlx::query_as(
            "SELECT id, url, label, agent_user, agent_password, agent_secret,
                    backup_confirmed, backup_confirmed_at, backup_reference,
                    wp_version, php_version, plugin_updates, core_update, status, last_checked
             FROM sites WHERE id = $1",
        )
        .bind(&id)
        .fetch_optional(&state.pool)
        .await?;

    let Some((id, url, label, user, password, secret, backup_confirmed, confirmed_at, reference, wp, php, updates, core_update, status, checked)) = row else {
        return Err(ApiError::NotFound(format!("Site {id} nicht gefunden")));
    };

    let credentials_complete = user.as_deref().map(str::trim).unwrap_or("").len() > 0
        && password.as_deref().map(str::trim).unwrap_or("").len() > 0
        && secret.as_deref().map(str::trim).unwrap_or("").len() > 0;

    Ok(Json(SiteAgentStatus {
        id,
        url,
        label,
        agent_user: user,
        credentials_complete,
        backup_confirmed,
        backup_confirmed_at: confirmed_at,
        backup_reference: reference,
        wp_version: wp,
        php_version: php,
        plugin_updates: updates,
        core_update,
        status,
        last_checked: checked,
    }))
}

pub async fn set_site_agent(
    State(state): State<SharedState>,
    user: CurrentUser,
    Path(id): Path<String>,
    Json(payload): Json<SiteAgentRequest>,
) -> ApiResult<Json<SiteAgentStatus>> {
    let exists: Option<String> = sqlx::query_scalar("SELECT id FROM sites WHERE id = $1")
        .bind(&id)
        .fetch_optional(&state.pool)
        .await?;

    if exists.is_none() {
        return Err(ApiError::NotFound(format!("Site {id} nicht gefunden")));
    }

    if let Some(user) = &payload.agent_user {
        sqlx::query("UPDATE sites SET agent_user = $1 WHERE id = $2")
            .bind(user.trim())
            .bind(&id)
            .execute(&state.pool)
            .await?;
    }

    if let Some(password) = &payload.agent_password {
        sqlx::query("UPDATE sites SET agent_password = $1 WHERE id = $2")
            .bind(password.trim())
            .bind(&id)
            .execute(&state.pool)
            .await?;
    }

    if let Some(secret) = &payload.agent_secret {
        sqlx::query("UPDATE sites SET agent_secret = $1 WHERE id = $2")
            .bind(secret.trim())
            .bind(&id)
            .execute(&state.pool)
            .await?;
    }

    if let Some(confirmed) = payload.backup_confirmed {
        sqlx::query(
            "UPDATE sites SET backup_confirmed = $1, backup_confirmed_at = $2, backup_reference = $3 WHERE id = $4",
        )
        .bind(confirmed)
        .bind(if confirmed { Some(now_iso()) } else { None })
        .bind(if confirmed { payload.backup_reference.clone() } else { None })
        .bind(&id)
        .execute(&state.pool)
        .await?;

        audit(
            &state.pool,
            user.actor(),
            if confirmed {
                "Backup für Site bestätigt"
            } else {
                "Backup-Bestätigung zurückgesetzt"
            },
            Some("site"),
            Some(&id),
            payload
                .backup_reference
                .as_deref()
                .unwrap_or("ohne Referenz"),
        )
        .await?;
    }

    site_agent_status(State(state), Path(id)).await
}

/// TeamViewer-ID eines Geräts pflegen. Leerer Wert löscht die Verknüpfung.
///
/// NinjaOne liefert die ID nicht mit, sie wird deshalb von Hand eingetragen.
/// Ziffern, Leerzeichen und Bindestriche sind erlaubt – gespeichert wird nur
/// die Ziffernfolge, weil genau die im URL-Schema erwartet wird.
pub async fn set_device_teamviewer(
    State(state): State<SharedState>,
    user: CurrentUser,
    Path(id): Path<String>,
    Json(payload): Json<TeamViewerRequest>,
) -> ApiResult<Json<Device>> {
    let raw = payload.teamviewer_id.unwrap_or_default();
    let digits: String = raw.chars().filter(|c| c.is_ascii_digit()).collect();

    if !raw.trim().is_empty() {
        let only_separators = raw
            .chars()
            .all(|c| c.is_ascii_digit() || c == ' ' || c == '-' || c == '.');
        if !only_separators || !(6..=12).contains(&digits.len()) {
            return Err(ApiError::BadRequest(
                "TeamViewer-ID muss aus 6 bis 12 Ziffern bestehen.".to_string(),
            ));
        }
    }

    let stored = if digits.is_empty() {
        None
    } else {
        Some(digits.clone())
    };

    let result = sqlx::query("UPDATE devices SET teamviewer_id = $1 WHERE id = $2")
        .bind(stored.as_deref())
        .bind(&id)
        .execute(&state.pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(ApiError::NotFound(format!("Gerät {id} nicht gefunden")));
    }

    audit(
        &state.pool,
        user.actor(),
        if stored.is_some() {
            "TeamViewer-ID gespeichert"
        } else {
            "TeamViewer-ID entfernt"
        },
        Some("device"),
        Some(&id),
        &match &stored {
            Some(digits) => format!("TeamViewer-ID {digits} hinterlegt."),
            None => "Verknüpfung zu TeamViewer gelöst.".to_string(),
        },
    )
    .await?;

    let device = sqlx::query_as::<_, Device>("SELECT * FROM devices WHERE id = $1")
        .bind(&id)
        .fetch_one(&state.pool)
        .await?;

    Ok(Json(device))
}

pub async fn set_customer_focus(
    State(state): State<SharedState>,
    user: CurrentUser,
    Path(id): Path<String>,
    Json(payload): Json<FocusRequest>,
) -> ApiResult<Json<CustomerSummary>> {
    let result = sqlx::query("UPDATE customers SET focus = $1 WHERE id = $2 AND active = true")
        .bind(payload.focus)
        .bind(&id)
        .execute(&state.pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(ApiError::NotFound(format!("Kunde {id} nicht gefunden")));
    }

    audit(
        &state.pool,
        user.actor(),
        if payload.focus {
            "Kunde in Fokus aufgenommen"
        } else {
            "Kunde aus Fokus genommen"
        },
        Some("customer"),
        Some(&id),
        if payload.focus {
            "Kunde erscheint wieder in Übersicht und Kennzahlen."
        } else {
            "Kunde bleibt verwaltet, wird aber in der Übersicht ausgeblendet."
        },
    )
    .await?;

    let month = current_month();
    let summary = sqlx::query_as::<_, CustomerSummary>(
        r#"
        SELECT
            c.id, c.name, c.short_code, c.ninja_org_id, c.contact_name, c.contact_email, c.phone,
            c.focus,
            (SELECT COUNT(*) FROM devices d WHERE d.customer_id = c.id) AS device_count,
            (SELECT COUNT(*) FROM sites s WHERE s.customer_id = c.id) AS site_count,
            (SELECT COUNT(*) FROM runs r WHERE r.customer_id = c.id AND r.status IN ('planned', 'in_progress', 'blocked')) AS open_runs,
            COALESCE((
                SELECT COALESCE(SUM(te.duration_seconds), 0)::double precision / 3600.0 FROM time_entries te
                WHERE te.customer_id = c.id AND te.billable = true
                  AND substr(te.started_at, 1, 7) = $1
            ), 0.0) AS month_hours,
            (SELECT COUNT(*) FROM sites s WHERE s.customer_id = c.id AND s.status IN ('warning', 'critical'))
              + (SELECT COUNT(*) FROM devices d WHERE d.customer_id = c.id
                   AND (d.patch_status = 'pending' OR d.antivirus_status = 'warning')) AS issues
        FROM customers c
        WHERE c.id = $2
        "#,
    )
    .bind(&month)
    .bind(&id)
    .fetch_one(&state.pool)
    .await?;

    Ok(Json(summary))
}

pub async fn list_runs(
    State(state): State<SharedState>,
    Query(filter): Query<RunFilter>,
) -> ApiResult<Json<Vec<RunListItem>>> {
    let runs = sqlx::query_as::<_, RunListItem>(
        r#"
        SELECT
            r.id, r.customer_id, c.name AS customer_name, c.short_code AS customer_short_code,
            r.period, r.title, r.scheduled_for, r.status, r.assignee,
            (SELECT COUNT(*) FROM tasks t WHERE t.run_id = r.id) AS total_tasks,
            (SELECT COUNT(*) FROM tasks t WHERE t.run_id = r.id AND t.status = 'done') AS done_tasks,
            r.kind, r.priority, r.description, r.due_at, r.channel
        FROM runs r
        JOIN customers c ON c.id = r.customer_id
        WHERE ($1 IS NULL OR r.status = $1)
          AND ($2 IS NULL OR r.customer_id = $2)
          AND ($3 IS NULL OR r.kind = $3)
          AND c.active = true
        ORDER BY
            CASE r.priority WHEN 'urgent' THEN 0 WHEN 'high' THEN 1 WHEN 'normal' THEN 2 ELSE 3 END,
            r.scheduled_for DESC
        "#,
    )
    .bind(filter.status.as_deref())
    .bind(filter.customer_id.as_deref())
    .bind(filter.kind.as_deref())
    .fetch_all(&state.pool)
    .await?;

    Ok(Json(runs))
}

pub async fn list_tickets(
    State(state): State<SharedState>,
    Query(filter): Query<RunFilter>,
) -> ApiResult<Json<Vec<RunListItem>>> {
    let scoped = RunFilter {
        status: filter.status,
        customer_id: filter.customer_id,
        kind: Some("ticket".to_string()),
    };
    list_runs(State(state), Query(scoped)).await
}

pub async fn create_ticket(
    State(state): State<SharedState>,
    user: CurrentUser,
    Json(payload): Json<CreateTicketRequest>,
) -> ApiResult<Json<CreateTicketResponse>> {
    let title = payload.title.trim();
    if title.is_empty() {
        return Err(ApiError::BadRequest("Bitte einen Titel angeben.".to_string()));
    }

    let priority = payload.priority.as_deref().unwrap_or("normal");
    if !matches!(priority, "low" | "normal" | "high" | "urgent") {
        return Err(ApiError::BadRequest(
            "Priorität muss low, normal, high oder urgent sein.".to_string(),
        ));
    }

    let customer: Option<(String, i64)> =
        sqlx::query_as("SELECT name, active FROM customers WHERE id = $1")
            .bind(&payload.customer_id)
            .fetch_optional(&state.pool)
            .await?;

    let Some((customer_name, active)) = customer else {
        return Err(ApiError::NotFound(format!(
            "Kunde {} nicht gefunden",
            payload.customer_id
        )));
    };

    if active == 0 {
        return Err(ApiError::BadRequest(
            "Für diesen Kunden ist keine Betreuung aktiv.".to_string(),
        ));
    }

    let (device_label, device_id) = match payload.device_id.as_deref() {
        Some(id) if !id.is_empty() => {
            let device: Option<(String, Option<String>, String)> =
                sqlx::query_as("SELECT hostname, os, customer_id FROM devices WHERE id = $1")
                    .bind(id)
                    .fetch_optional(&state.pool)
                    .await?;

            let Some((hostname, os, device_customer)) = device else {
                return Err(ApiError::NotFound(format!("Gerät {id} nicht gefunden")));
            };

            if device_customer != payload.customer_id {
                return Err(ApiError::BadRequest(
                    "Das Gerät gehört nicht zu diesem Kunden.".to_string(),
                ));
            }

            let label = match os {
                Some(os) => format!("{hostname} · {os}"),
                None => hostname,
            };
            (Some(label), Some(id.to_string()))
        }
        _ => (None, None),
    };

    let ticket_id = Uuid::new_v4().to_string();
    let task_id = Uuid::new_v4().to_string();
    let now = now_iso();
    let period = current_month();
    let assignee = payload
        .assignee
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(user.actor());

    sqlx::query(
        "INSERT INTO runs
         (id, customer_id, contract_id, period, title, scheduled_for, status, assignee, notes, created_at, completed_at, kind, priority, description, channel, due_at)
         VALUES ($1, $2, NULL, $3, $4, $5, 'planned', $6, NULL, $5, NULL, 'ticket', $7, $8, $9, $10)",
    )
    .bind(&ticket_id)
    .bind(&payload.customer_id)
    .bind(&period)
    .bind(title)
    .bind(&now)
    .bind(assignee)
    .bind(priority)
    .bind(payload.description.as_deref())
    .bind(payload.channel.as_deref())
    .bind(payload.due_at.as_deref())
    .execute(&state.pool)
    .await?;

    sqlx::query(
        "INSERT INTO tasks
         (id, run_id, position, title, description, kind, target_type, target_id, target_label, status, risk, requires_approval, approved, result_summary, started_at, finished_at)
         VALUES ($1, $2, 1, 'Bearbeitung dokumentieren', $3, 'MANUAL', $4, $5, $6, 'open', 'low', false, false, NULL, NULL, NULL)",
    )
    .bind(&task_id)
    .bind(&ticket_id)
    .bind(payload.description.as_deref())
    .bind(if device_id.is_some() { Some("device") } else { None })
    .bind(device_id.as_deref())
    .bind(device_label.as_deref())
    .execute(&state.pool)
    .await?;

    audit(
        &state.pool,
        assignee,
        "Einsatz angelegt",
        Some("ticket"),
        Some(&ticket_id),
        &format!("{title} · {customer_name} · Priorität {priority}"),
    )
    .await?;

    Ok(Json(CreateTicketResponse {
        ticket_id,
        task_id,
        title: title.to_string(),
    }))
}

pub async fn create_run(
    State(state): State<SharedState>,
    user: CurrentUser,
    Json(payload): Json<CreateRunRequest>,
) -> ApiResult<Json<CreateRunResponse>> {
    // WordPress-Vorlagen zielen auf eine Site, nicht auf ein Gerät.
    if matches!(payload.template.as_str(), "wp-health" | "wp-plugins" | "wp-core") {
        return create_site_run(state, user.actor(), payload).await;
    }

    let customer: Option<(String, i64)> =
        sqlx::query_as("SELECT name, active FROM customers WHERE id = $1")
            .bind(&payload.customer_id)
            .fetch_optional(&state.pool)
            .await?;

    let Some((customer_name, active)) = customer else {
        return Err(ApiError::NotFound(format!(
            "Kunde {} nicht gefunden",
            payload.customer_id
        )));
    };

    if active == 0 {
        return Err(ApiError::BadRequest(
            "Für diesen Kunden ist keine Wartung aktiv.".to_string(),
        ));
    }

    let device: Option<(String, Option<String>, String, Option<i64>)> = sqlx::query_as(
        "SELECT hostname, os, customer_id, ninja_device_id FROM devices WHERE id = $1",
    )
    .bind(&payload.device_id)
    .fetch_optional(&state.pool)
    .await?;

    let Some((hostname, os, device_customer, ninja_device_id)) = device else {
        return Err(ApiError::NotFound(format!(
            "Gerät {} nicht gefunden",
            payload.device_id
        )));
    };

    if device_customer != payload.customer_id {
        return Err(ApiError::BadRequest(
            "Das Gerät gehört nicht zu diesem Kunden.".to_string(),
        ));
    }

    let ninja_device_id = ninja_device_id.ok_or_else(|| {
        ApiError::BadRequest("Das Gerät ist nicht mit NinjaOne verknüpft.".to_string())
    })?;

    let (run_title, task_title, task_kind, requires_approval, risk, description) =
        match payload.template.as_str() {
            "script" => (
                format!("Schnelltest – {hostname}"),
                format!("Script-Test auf {hostname}"),
                "NINJA_SCRIPT",
                true,
                "low",
                "Führt eine eingebaute NinjaOne-Aktion auf dem Gerät aus (Standard: DNS-Cache leeren) und übernimmt das Ergebnis aus der Gerätehistorie.",
            ),
            "patch-scan" => (
                format!("Schnelltest – {hostname}"),
                format!("Patch-Scan auf {hostname}"),
                "NINJA_PATCH",
                false,
                "low",
                "Stößt einen OS-Patch-Scan in NinjaOne an. Es werden keine Patches installiert.",
            ),
            other => {
                return Err(ApiError::BadRequest(format!(
                    "Unbekannte Vorlage: {other}. Erlaubt sind script und patch-scan."
                )))
            }
        };

    let run_id = Uuid::new_v4().to_string();
    let task_id = Uuid::new_v4().to_string();
    let now = now_iso();
    let period = current_month();
    let target_label = match os {
        Some(os) => format!("{hostname} · {os}"),
        None => hostname.clone(),
    };

    sqlx::query(
        "INSERT INTO runs (id, customer_id, contract_id, period, title, scheduled_for, status, assignee, notes, created_at, completed_at)
         VALUES ($1, $2, NULL, $3, $4, $5, 'planned', $6, $7, $5, NULL)",
    )
    .bind(&run_id)
    .bind(&payload.customer_id)
    .bind(&period)
    .bind(&run_title)
    .bind(&now)
    .bind(user.actor())
    .bind(payload.note.as_deref())
    .execute(&state.pool)
    .await?;

    sqlx::query(
        "INSERT INTO tasks
         (id, run_id, position, title, description, kind, target_type, target_id, target_label, status, risk, requires_approval, approved, result_summary, started_at, finished_at)
         VALUES ($1, $2, 1, $3, $4, $5, 'device', $6, $7, 'open', $8, $9, false, NULL, NULL, NULL)",
    )
    .bind(&task_id)
    .bind(&run_id)
    .bind(&task_title)
    .bind(description)
    .bind(task_kind)
    .bind(&payload.device_id)
    .bind(&target_label)
    .bind(risk)
    .bind(requires_approval)
    .execute(&state.pool)
    .await?;

    audit(
        &state.pool,
        user.actor(),
        "Schnelltest angelegt",
        Some("run"),
        Some(&run_id),
        &format!(
            "{task_title} · NinjaOne-Gerät {ninja_device_id} · Kunde {customer_name}"
        ),
    )
    .await?;

    Ok(Json(CreateRunResponse {
        run_id,
        task_id,
        title: task_title,
        requires_approval,
    }))
}

/// Legt einen Testlauf für eine WordPress-Site an.
async fn create_site_run(
    state: SharedState,
    actor: &str,
    payload: CreateRunRequest,
) -> ApiResult<Json<CreateRunResponse>> {
    let site: Option<(String, Option<String>, String, Option<String>)> =
        sqlx::query_as("SELECT url, label, customer_id, agent_secret FROM sites WHERE id = $1")
            .bind(&payload.device_id)
            .fetch_optional(&state.pool)
            .await?;

    let Some((url, label, customer_id, secret)) = site else {
        return Err(ApiError::NotFound(format!(
            "Site {} nicht gefunden",
            payload.device_id
        )));
    };

    if secret.as_deref().map(str::trim).unwrap_or("").is_empty() {
        return Err(ApiError::BadRequest(
            "Für diese Site sind keine Agent-Zugangsdaten hinterlegt.".to_string(),
        ));
    }

    if payload.customer_id != customer_id {
        return Err(ApiError::BadRequest(
            "Die Site gehört nicht zu diesem Kunden.".to_string(),
        ));
    }

    let host = url
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_end_matches('/')
        .to_string();
    let display = label.unwrap_or_else(|| host.clone());

    let (task_title, task_kind, requires_approval, risk, description) = match payload.template.as_str() {
        "wp-health" => (
            format!("Health-Check auf {display}"),
            "WP_HEALTH",
            false,
            "low",
            "Prüft Versionen, Plugin-Zustand und Erreichbarkeit der Site.",
        ),
        "wp-plugins" => (
            format!("Plugin-Updates auf {display}"),
            "WP_PLUGIN_UPDATE",
            true,
            "medium",
            "Spielt alle verfügbaren Plugin-Updates ein. Erst Dry-Run, danach echtes Update mit Health-Check.",
        ),
        _ => (
            format!("Core-Update auf {display}"),
            "WP_CORE_UPDATE",
            true,
            "high",
            "Aktualisiert den WordPress-Core. Nur mit bestätigtem Backup.",
        ),
    };

    let run_id = Uuid::new_v4().to_string();
    let task_id = Uuid::new_v4().to_string();
    let now = now_iso();
    let period = current_month();

    sqlx::query(
        "INSERT INTO runs (id, customer_id, contract_id, period, title, scheduled_for, status, assignee, notes, created_at, completed_at, kind)
         VALUES ($1, $2, NULL, $3, $4, $5, 'planned', $6, $7, $5, NULL, 'maintenance')",
    )
    .bind(&run_id)
    .bind(&payload.customer_id)
    .bind(&period)
    .bind(&task_title)
    .bind(&now)
    .bind(actor)
    .bind(payload.note.as_deref())
    .execute(&state.pool)
    .await?;

    sqlx::query(
        "INSERT INTO tasks
         (id, run_id, position, title, description, kind, target_type, target_id, target_label, status, risk, requires_approval, approved, result_summary, started_at, finished_at)
         VALUES ($1, $2, 1, $3, $4, $5, 'site', $6, $7, 'open', $8, $9, false, NULL, NULL, NULL)",
    )
    .bind(&task_id)
    .bind(&run_id)
    .bind(&task_title)
    .bind(description)
    .bind(task_kind)
    .bind(&payload.device_id)
    .bind(format!("{display} · {url}"))
    .bind(risk)
    .bind(requires_approval)
    .execute(&state.pool)
    .await?;

    audit(
        &state.pool,
        actor,
        "WordPress-Testlauf angelegt",
        Some("run"),
        Some(&run_id),
        &format!("{task_title} · {url}"),
    )
    .await?;

    Ok(Json(CreateRunResponse {
        run_id,
        task_id,
        title: task_title,
        requires_approval,
    }))
}

/// Schnellaktion auf einem Gerät: Patch-Scan oder Script.
///
/// Legt für den Nachweis einen Wartungslauf mit einer Aufgabe an, gibt
/// aber sofort die Job-ID zurück, damit die Oberfläche den Fortschritt
/// direkt im Geräte-Panel anzeigen kann – ohne Seitenwechsel.
pub async fn quick_device_action(
    State(state): State<SharedState>,
    user: CurrentUser,
    Path(device_id): Path<String>,
    Json(payload): Json<QuickActionRequest>,
) -> ApiResult<Json<QuickActionResponse>> {
    let device: Option<(String, Option<String>, Option<i64>, String, bool)> = sqlx::query_as(
        "SELECT hostname, os, ninja_device_id, customer_id, online FROM devices WHERE id = $1",
    )
    .bind(&device_id)
    .fetch_optional(&state.pool)
    .await?;

    let Some((hostname, os, ninja_device_id, customer_id, online)) = device else {
        return Err(ApiError::NotFound(format!("Gerät {device_id} nicht gefunden")));
    };

    if ninja_device_id.is_none() {
        return Err(ApiError::BadRequest(
            "Dieses Gerät ist nicht mit NinjaOne verknüpft.".to_string(),
        ));
    }

    // Ein Gerät, ein Vorgang: Zwei gleichzeitige Patch-Läufe blockieren sich in
    // NinjaOne gegenseitig und der Nachweis wäre nicht mehr zuzuordnen.
    let running: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM jobs j
         JOIN tasks t ON t.id = j.task_id
         WHERE t.target_id = $1 AND j.status IN ('queued', 'running')",
    )
    .bind(&device_id)
    .fetch_one(&state.pool)
    .await?;

    if running > 0 {
        return Err(ApiError::Conflict(
            "Auf diesem Gerät läuft gerade ein Vorgang. Bitte warten, bis er abgeschlossen ist."
                .to_string(),
        ));
    }

    let (task_title, task_kind, action, risk, description) = match payload.kind.as_str() {
        "patch-scan" => (
            format!("OS-Scan auf {hostname}"),
            "NINJA_PATCH",
            "patch_scan_os",
            "low",
            "Lässt in NinjaOne den Patchbestand des Betriebssystems neu prüfen. Es wird nichts installiert.",
        ),
        "software-scan" => (
            format!("Software-Scan auf {hostname}"),
            "NINJA_PATCH",
            "patch_scan_software",
            "low",
            "Lässt in NinjaOne den Bestand installierter Programme neu prüfen. Es wird nichts installiert.",
        ),
        "patch-apply" => (
            format!("OS-Updates auf {hostname}"),
            "NINJA_PATCH",
            "patch_apply_os",
            "normal",
            "Installiert in NinjaOne alle genehmigten Betriebssystem-Updates. Ein Neustart kann nötig werden.",
        ),
        "software-apply" => (
            format!("Software-Updates auf {hostname}"),
            "NINJA_PATCH",
            "patch_apply_software",
            "normal",
            "Installiert in NinjaOne alle genehmigten Software-Updates. Anwendungen können sich dabei schließen.",
        ),
        "reboot" => {
            let forced = payload.forced.unwrap_or(false);
            (
                if forced {
                    format!("Neustart (erzwungen) auf {hostname}")
                } else {
                    format!("Neustart auf {hostname}")
                },
                "NINJA_REBOOT",
                if forced { "reboot_forced" } else { "reboot_normal" },
                "normal",
                if forced {
                    "Startet das Gerät neu und schließt dabei Anwendungen ohne Nachfrage."
                } else {
                    "Startet das Gerät neu. Offene Anwendungen können Anwender zum Speichern auffordern."
                },
            )
        }
        "maintenance-start" => (
            format!("Wartungsmodus auf {hostname}"),
            "NINJA_MAINTENANCE",
            "maintenance_start",
            "low",
            "Setzt in NinjaOne ein Wartungsfenster: Alarme bleiben währenddessen unterdrückt.",
        ),
        "maintenance-stop" => (
            format!("Wartungsmodus beenden auf {hostname}"),
            "NINJA_MAINTENANCE",
            "maintenance_stop",
            "low",
            "Beendet das Wartungsfenster vorzeitig. Alarme greifen sofort wieder.",
        ),
        "script" => (
            format!("Script-Test auf {hostname}"),
            "NINJA_SCRIPT",
            "",
            "low",
            "Führt eine eingebaute NinjaOne-Aktion auf dem Gerät aus (Standard: DNS-Cache leeren).",
        ),
        other => {
            return Err(ApiError::BadRequest(format!(
                "Unbekannte Aktion: {other}. Erlaubt sind patch-scan, software-scan, \
                 patch-apply, software-apply, reboot, maintenance-start, maintenance-stop und script."
            )))
        }
    };

    // NinjaOne nimmt Aufträge für offline Geräte an und führt sie später aus –
    // für einen wartenden Techniker ist das aber eine falsche Erwartung.
    let needs_online = action != "maintenance_stop";
    if needs_online && !online {
        return Err(ApiError::Conflict(format!(
            "{hostname} ist offline. Der Auftrag würde erst beim nächsten Kontakt ausgeführt. \
             In NinjaOne findest du unter diesem Gerät den letzten Kontakt."
        )));
    }

    let minutes = payload.minutes.unwrap_or(60).clamp(1, 24 * 60);
    let action_params = match action {
        "reboot_normal" | "reboot_forced" => serde_json::json!({
            "reason": payload.reason.clone().unwrap_or_else(|| "Wartung über Wartungscockpit".to_string()),
        }),
        "maintenance_start" => serde_json::json!({
            "minutes": minutes,
            "features": ["ALERTS"],
        }),
        _ => serde_json::json!({}),
    };

    let task_id = Uuid::new_v4().to_string();
    let now = now_iso();
    let period = current_month();

    let target_label = match os {
        Some(os) => format!("{hostname} · {os}"),
        None => hostname.clone(),
    };

    // Sofortaktionen sammeln sich in einem Lauf pro Kunde und Monat. Sonst
    // erzeugt jeder Klick auf „OS-Scan" einen eigenen Wartungslauf und die
    // Liste füllt sich mit Einzeilern. Der Lauf darf schon abgeschlossen sein –
    // eine neue Aktion hängt sich an und öffnet ihn wieder.
    let existing_run: Option<String> = sqlx::query_scalar(
        "SELECT id FROM runs
         WHERE customer_id = $1 AND period = $2 AND kind = 'quick'
         ORDER BY created_at DESC LIMIT 1",
    )
    .bind(&customer_id)
    .bind(&period)
    .fetch_optional(&state.pool)
    .await?;

    let run_id = match existing_run {
        Some(id) => id,
        None => {
            let new_id = Uuid::new_v4().to_string();
            sqlx::query(
                "INSERT INTO runs (id, customer_id, contract_id, period, title, scheduled_for, status, assignee, notes, created_at, completed_at, kind)
                 VALUES ($1, $2, NULL, $3, $4, $5, 'planned', $6, NULL, $5, NULL, 'quick')",
            )
            .bind(&new_id)
            .bind(&customer_id)
            .bind(&period)
            .bind(format!("Sofortaktionen {}", month_label(&period)))
            .bind(&now)
            .bind(user.actor())
            .execute(&state.pool)
            .await?;
            new_id
        }
    };

    let position: i64 = sqlx::query_scalar("SELECT COALESCE(MAX(position), 0) + 1 FROM tasks WHERE run_id = $1")
        .bind(&run_id)
        .fetch_one(&state.pool)
        .await?;

    // approved = true: der ausdrückliche Klick auf die Aktion ist die Freigabe.
    sqlx::query(
        "INSERT INTO tasks
         (id, run_id, position, title, description, kind, action, action_params, target_type, target_id, target_label, status, risk, requires_approval, approved, result_summary, started_at, finished_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'device', $9, $10, 'open', $11, false, true, NULL, NULL, NULL)",
    )
    .bind(&task_id)
    .bind(&run_id)
    .bind(position)
    .bind(&task_title)
    .bind(description)
    .bind(task_kind)
    .bind(if action.is_empty() { None } else { Some(action) })
    .bind(action_params.to_string())
    .bind(&device_id)
    .bind(&target_label)
    .bind(risk)
    .execute(&state.pool)
    .await?;

    let task = fetch_task(&state.pool, &task_id).await?;
    let execution = start_execution(&state, &task, user.actor()).await?;

    Ok(Json(QuickActionResponse {
        run_id,
        task_id,
        job_id: execution.job_id,
        execution_id: execution.execution_id,
        title: task_title,
    }))
}

pub async fn get_run(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> ApiResult<Json<RunDetail>> {
    let run = sqlx::query_as::<_, Run>("SELECT * FROM runs WHERE id = $1")
        .bind(&id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Wartungslauf {id} nicht gefunden")))?;

    let customer = sqlx::query_as::<_, Customer>(
        "SELECT id, name, short_code, ninja_org_id, contact_name, contact_email, phone, notes, active, focus
         FROM customers WHERE id = $1",
    )
    .bind(&run.customer_id)
    .fetch_one(&state.pool)
    .await?;

    let tasks = sqlx::query_as::<_, Task>(
        "SELECT * FROM tasks WHERE run_id = $1 ORDER BY position",
    )
    .bind(&id)
    .fetch_all(&state.pool)
    .await?;

    let executions = sqlx::query_as::<_, ExecutionRecord>(
        r#"
        SELECT e.id, e.task_id, e.job_id, e.status, e.request, e.response, e.error,
               e.duration_ms, e.before_version, e.after_version, e.created_at
        FROM task_executions e
        JOIN tasks t ON t.id = e.task_id
        WHERE t.run_id = $1
        ORDER BY e.created_at DESC
        "#,
    )
    .bind(&id)
    .fetch_all(&state.pool)
    .await?;

    let total_time_seconds: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(duration_seconds), 0)::bigint FROM time_entries WHERE run_id = $1 AND source != 'machine'",
    )
    .bind(&id)
    .fetch_one(&state.pool)
    .await?;

    let machine_time_seconds: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(duration_seconds), 0)::bigint FROM time_entries WHERE run_id = $1 AND source = 'machine'",
    )
    .bind(&id)
    .fetch_one(&state.pool)
    .await?;

    Ok(Json(RunDetail {
        run,
        customer,
        tasks,
        executions,
        total_time_seconds,
        machine_time_seconds,
    }))
}

pub async fn get_task(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> ApiResult<Json<Task>> {
    Ok(Json(fetch_task(&state.pool, &id).await?))
}

pub async fn approve_task(
    State(state): State<SharedState>,
    user: CurrentUser,
    Path(id): Path<String>,
) -> ApiResult<Json<Task>> {
    let result = sqlx::query("UPDATE tasks SET approved = true WHERE id = $1")
        .bind(&id)
        .execute(&state.pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(ApiError::NotFound(format!("Aufgabe {id} nicht gefunden")));
    }

    let task = fetch_task(&state.pool, &id).await?;

    audit(
        &state.pool,
        user.actor(),
        "Aufgabe freigegeben",
        Some("task"),
        Some(&id),
        &format!("Freigabe für: {}", task.title),
    )
    .await?;

    Ok(Json(task))
}

pub async fn execute_task(
    State(state): State<SharedState>,
    user: CurrentUser,
    Path(id): Path<String>,
) -> ApiResult<Json<ExecuteResponse>> {
    let task = fetch_task(&state.pool, &id).await?;

    if task.status == "running" {
        return Err(ApiError::Conflict(
            "Diese Aufgabe wird bereits ausgeführt.".to_string(),
        ));
    }

    if task.status == "done" {
        return Err(ApiError::Conflict(
            "Diese Aufgabe ist bereits abgeschlossen. Für einen erneuten Lauf bitte eine neue Aufgabe anlegen."
                .to_string(),
        ));
    }

    if task.requires_approval && !task.approved {
        return Err(ApiError::Conflict(
            "Diese Aufgabe ist als risikoreich markiert. Bitte zuerst freigeben.".to_string(),
        ));
    }

    let response = start_execution(&state, &task, user.actor()).await?;
    Ok(Json(response))
}

/// Legt Job und Ausführungsprotokoll an und startet den Worker.
async fn start_execution(
    state: &SharedState,
    task: &Task,
    actor: &str,
) -> ApiResult<ExecuteResponse> {
    let job_id = Uuid::new_v4().to_string();
    let execution_id = Uuid::new_v4().to_string();
    let now = now_iso();

    sqlx::query(
        "UPDATE tasks SET status = 'running', started_at = $1, finished_at = NULL, result_summary = NULL WHERE id = $2",
    )
    .bind(&now)
    .bind(&task.id)
    .execute(&state.pool)
    .await?;

    sqlx::query(
        "INSERT INTO jobs (id, task_id, kind, status, progress, message, created_at, updated_at)
         VALUES ($1, $2, $3, 'queued', 0, $4, $5, $5)",
    )
    .bind(&job_id)
    .bind(&task.id)
    .bind(&task.kind)
    .bind("Aufgabe wird vorbereitet")
    .bind(&now)
    .execute(&state.pool)
    .await?;

    sqlx::query(
        "INSERT INTO task_executions (id, task_id, job_id, status, request, created_at)
         VALUES ($1, $2, $3, 'running', $4, $5)",
    )
    .bind(&execution_id)
    .bind(&task.id)
    .bind(&job_id)
    .bind(serde_json::json!({ "kind": task.kind, "target": task.target_label }).to_string())
    .bind(&now)
    .execute(&state.pool)
    .await?;

    sqlx::query(
        "UPDATE runs SET status = 'in_progress' WHERE id = $1 AND status IN ('planned', 'blocked', 'done')",
    )
    .bind(&task.run_id)
    .execute(&state.pool)
    .await?;

    audit(
        &state.pool,
        actor,
        "Aufgabe gestartet",
        Some("task"),
        Some(&task.id),
        &format!("Ausführung gestartet: {}", task.title),
    )
    .await?;

    let task_for_worker = task.clone();
    let state_for_worker = state.clone();
    let job_for_worker = job_id.clone();
    let execution_for_worker = execution_id.clone();

    tokio::spawn(async move {
        executors::execute_task(
            state_for_worker,
            task_for_worker,
            job_for_worker,
            execution_for_worker,
        )
        .await;
    });

    Ok(ExecuteResponse {
        job_id,
        execution_id,
    })
}

pub async fn get_job(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> ApiResult<Json<Job>> {
    let job = sqlx::query_as::<_, Job>("SELECT * FROM jobs WHERE id = $1")
        .bind(&id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Job {id} nicht gefunden")))?;

    Ok(Json(job))
}

/// Startet den Benutzer-OAuth-Flow (App-Typ "Webanwendung").
pub async fn ninjaone_auth_start(
    State(state): State<SharedState>,
) -> ApiResult<Json<serde_json::Value>> {
    if !state.ninja.user_app_configured() {
        return Err(ApiError::BadRequest(
            "Keine Benutzer-App konfiguriert: NINJAONE_USER_CLIENT_ID und NINJAONE_USER_CLIENT_SECRET in .env setzen (App-Typ Webanwendung)."
                .to_string(),
        ));
    }

    let oauth_state = Uuid::new_v4().to_string();

    sqlx::query(
        "INSERT INTO app_settings (key, value) VALUES ('ninjaone.oauth_state', $1)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
    )
    .bind(&oauth_state)
    .execute(&state.pool)
    .await?;

    let url = state
        .ninja
        .authorize_url(&oauth_state)
        .ok_or_else(|| ApiError::BadRequest("Autorisierungs-URL nicht verfügbar.".to_string()))?;

    Ok(Json(serde_json::json!({ "url": url })))
}

/// Nimmt den Authorization Code entgegen und speichert den Refresh-Token.
pub async fn ninjaone_auth_callback(
    State(state): State<SharedState>,
    Query(query): Query<OAuthCallbackQuery>,
) -> impl IntoResponse {
    if let Some(error) = query.error {
        let detail = query.error_description.unwrap_or_default();
        return Html(auth_page(
            "Verbindung abgelehnt",
            &format!("NinjaOne meldet: {error}. {detail}"),
            false,
        ));
    }

    let Some(code) = query.code else {
        return Html(auth_page(
            "Kein Autorisierungscode",
            "Die Antwort enthielt keinen Code. Bitte den Vorgang erneut starten.",
            false,
        ));
    };

    let expected: Option<String> = sqlx::query_scalar(
        "SELECT value FROM app_settings WHERE key = 'ninjaone.oauth_state'",
    )
    .fetch_optional(&state.pool)
    .await
    .ok()
    .flatten();

    if expected.is_some() && query.state != expected {
        return Html(auth_page(
            "Ungültiger Zustand",
            "Der state-Parameter passt nicht. Bitte den Vorgang erneut starten.",
            false,
        ));
    }

    match state.ninja.exchange_code(&code).await {
        Ok(tokens) => {
            if tokens.refresh_token.is_empty() {
                return Html(auth_page(
                    "Kein Refresh-Token",
                    &format!(
                        "NinjaOne hat keinen Refresh-Token geliefert. In der App muss der Grant-Typ <strong>Aktualisierungstoken</strong> angehakt sein. Gelieferte Felder: <code>{}</code>",
                        tokens.field_names.join(", ")
                    ),
                    false,
                ));
            }

            if let Err(error) = sqlx::query(
                "INSERT INTO app_settings (key, value) VALUES ('ninjaone.user_refresh_token', $1)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            )
            .bind(&tokens.refresh_token)
            .execute(&state.pool)
            .await
            {
                tracing::error!(?error, "Refresh-Token konnte nicht gespeichert werden");
                return Html(auth_page(
                    "Speichern fehlgeschlagen",
                    "Der Token konnte nicht gespeichert werden. Details stehen im Server-Log.",
                    false,
                ));
            }

            state
                .ninja
                .set_user_refresh_token(Some(tokens.refresh_token.clone()))
                .await;

            let _ = audit(
                &state.pool,
                "System",
                "NinjaOne-Benutzer verbunden",
                Some("integration"),
                None,
                "Benutzerkontext für Script-Ausführung hinterlegt.",
            )
            .await;

            Html(auth_page(
                "NinjaOne-Benutzer verbunden",
                "Script-Ausführung ist jetzt möglich. Dieses Fenster kann geschlossen werden.",
                true,
            ))
        }
        Err(error) => {
            tracing::error!(?error, "Code-Tausch fehlgeschlagen");
            Html(auth_page("Verbindung fehlgeschlagen", &format!("{error}"), false))
        }
    }
}

fn auth_page(title: &str, message: &str, success: bool) -> String {
    let color = if success { "#0f766e" } else { "#b42318" };
    format!(
        "<!doctype html><html lang=\"de\"><head><meta charset=\"utf-8\"><title>{title}</title><style>body{{font-family:system-ui,sans-serif;background:#f4f6f8;display:grid;place-items:center;height:100vh;margin:0}}.card{{background:#fff;border:1px solid #e4e7ec;border-radius:16px;padding:36px;max-width:520px;box-shadow:0 16px 40px -20px rgba(16,24,40,.3)}}h1{{color:{color};font-size:22px;margin:0 0 10px}}p{{color:#475467;line-height:1.6;margin:0}}a{{display:inline-block;margin-top:20px;color:#0f766e;font-weight:600}}</style></head><body><div class=\"card\"><h1>{title}</h1><p>{message}</p><a href=\"/\">Zurück zum Wartungscockpit</a></div></body></html>"
    )
}

#[derive(Debug, Deserialize)]
pub struct ReportFilter {
    pub month: Option<String>,
    pub include_idle: Option<bool>,
}

pub async fn monthly_report(
    State(state): State<SharedState>,
    user: CurrentUser,
    Query(filter): Query<ReportFilter>,
) -> ApiResult<Json<crate::reports::MonthlyReport>> {
    let month = filter.month.unwrap_or_else(current_month);

    if !crate::reports::valid_month(&month) {
        return Err(ApiError::BadRequest(
            "Monat muss im Format YYYY-MM liegen, Monat 01 bis 12.".to_string(),
        ));
    }

    let billing = crate::settings::billing(&state.pool).await;
    let mut report = crate::reports::monthly(
        &state.pool,
        &month,
        filter.include_idle.unwrap_or(false),
        &billing,
    )
    .await?;

    report.generated_by = user.display_name.clone();

    audit(
        &state.pool,
        user.actor(),
        "Monatsbericht erzeugt",
        Some("report"),
        None,
        &format!(
            "Zeitraum {month}, {} Kunden, Summe {:.2} EUR",
            report.totals.customers, report.totals.total_amount
        ),
    )
    .await?;

    Ok(Json(report))
}

/// Gespeicherte Patches eines Geräts: zuerst offene, dann abgelehnte.
pub async fn device_patches(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> ApiResult<Json<Vec<DevicePatch>>> {
    let exists: Option<String> = sqlx::query_scalar("SELECT id FROM devices WHERE id = $1")
        .bind(&id)
        .fetch_optional(&state.pool)
        .await?;

    if exists.is_none() {
        return Err(ApiError::NotFound(format!("Gerät {id} nicht gefunden")));
    }

    let patches = sqlx::query_as::<_, DevicePatch>(
        r#"
        SELECT patch_id, name, kb_number, severity, status, patch_type, patch_kind, observed_at
        FROM device_patches
        WHERE device_id = $1
        ORDER BY
            patch_kind,
            CASE status
                WHEN 'APPROVED' THEN 0
                WHEN 'DELAYED' THEN 1
                WHEN 'MANUAL' THEN 2
                WHEN 'REJECTED' THEN 3
                ELSE 4
            END,
            name
        "#,
    )
    .bind(&id)
    .fetch_all(&state.pool)
    .await?;

    Ok(Json(patches))
}

/// Aktuelle Stundensatz- und Steuereinstellungen.

// ---------------------------------------------------------------------------
// Datenbankverbindung
// ---------------------------------------------------------------------------

/// Zeigt die Verbindungsdaten für die Datenbank. Das Passwort wird nie
/// zurückgegeben – nur, ob eines hinterlegt ist.
fn database_payload(config: &crate::dbconfig::DbConfig) -> serde_json::Value {
    serde_json::json!({
        "host": config.host,
        "port": config.port,
        "database": config.database,
        "user": config.user,
        "password_set": !config.password.is_empty(),
        "file": crate::dbconfig::DbConfig::path().display().to_string(),
    })
}

pub async fn get_database_settings(
    State(_state): State<SharedState>,
    user: CurrentUser,
) -> ApiResult<Json<serde_json::Value>> {
    if !user.role.is_admin() {
        return Err(ApiError::Forbidden(
            "Nur Administratoren dürfen die Datenbankverbindung sehen.".to_string(),
        ));
    }

    let (config, _source) = crate::dbconfig::DbConfig::load();
    Ok(Json(database_payload(&config)))
}

#[derive(Debug, serde::Deserialize)]
pub struct DatabaseSettingsRequest {
    pub host: String,
    pub port: u16,
    pub database: String,
    pub user: String,
    /// Leer lassen heißt: hinterlegtes Passwort behalten.
    #[serde(default)]
    pub password: Option<String>,
    /// Nur prüfen, nicht speichern.
    #[serde(default)]
    pub test_only: bool,
}

pub async fn update_database_settings(
    State(_state): State<SharedState>,
    user: CurrentUser,
    Json(payload): Json<DatabaseSettingsRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    if !user.role.is_admin() {
        return Err(ApiError::Forbidden(
            "Nur Administratoren dürfen die Datenbankverbindung ändern.".to_string(),
        ));
    }

    let (current, _source) = crate::dbconfig::DbConfig::load();
    let password = match payload.password.as_deref() {
        Some(value) if !value.is_empty() => value.to_string(),
        _ => current.password.clone(),
    };

    let candidate = crate::dbconfig::DbConfig {
        host: payload.host.trim().to_string(),
        port: payload.port,
        database: payload.database.trim().to_string(),
        user: payload.user.trim().to_string(),
        password,
    };

    if candidate.host.is_empty() || candidate.database.is_empty() || candidate.user.is_empty() {
        return Err(ApiError::BadRequest(
            "Server, Datenbank und Benutzer dürfen nicht leer sein.".to_string(),
        ));
    }

    // Erst verbinden, dann speichern: Eine falsche Adresse soll den nächsten
    // Start nicht unmöglich machen.
    let report = crate::db::test(&candidate.url())
        .await
        .map_err(|error| {
            ApiError::BadRequest(format!(
                "Verbindung fehlgeschlagen – nichts gespeichert: {error}"
            ))
        })?;

    if payload.test_only {
        return Ok(Json(serde_json::json!({ "ok": true, "report": report })));
    }

    candidate
        .save()
        .map_err(|error| ApiError::BadRequest(format!("Datei nicht schreibbar: {error}")))?;

    audit(
        &_state.pool,
        user.actor(),
        "Datenbankverbindung geändert",
        Some("app_settings"),
        None,
        &format!("neu: {}", candidate.describe()),
    )
    .await?;

    Ok(Json(serde_json::json!({
        "ok": true,
        "report": report,
        "restart_required": true,
        "settings": database_payload(&candidate),
    })))
}

pub async fn test_database_settings(
    user: CurrentUser,
) -> ApiResult<Json<serde_json::Value>> {
    if !user.role.is_admin() {
        return Err(ApiError::Forbidden(
            "Nur Administratoren dürfen die Datenbankverbindung prüfen.".to_string(),
        ));
    }

    let (config, _source) = crate::dbconfig::DbConfig::load();
    match crate::db::test(&config.url()).await {
        Ok(report) => Ok(Json(serde_json::json!({ "ok": true, "report": report }))),
        Err(error) => Ok(Json(serde_json::json!({ "ok": false, "report": error }))),
    }
}

// ---------------------------------------------------------------------------
// Umgebung (.env): NinjaOne-Zugänge
// ---------------------------------------------------------------------------

/// Zuordnung zwischen Oberflächenfeld und Schlüssel in der `.env`.
const ENV_FIELDS: [(&str, &str); 7] = [
    ("base_url", "NINJAONE_BASE_URL"),
    ("client_id", "NINJAONE_CLIENT_ID"),
    ("client_secret", "NINJAONE_CLIENT_SECRET"),
    ("scope", "NINJAONE_SCOPE"),
    ("user_client_id", "NINJAONE_USER_CLIENT_ID"),
    ("user_client_secret", "NINJAONE_USER_CLIENT_SECRET"),
    ("redirect_uri", "NINJAONE_REDIRECT_URI"),
];

fn env_payload(values: &std::collections::BTreeMap<String, String>) -> serde_json::Value {
    let mut fields = serde_json::Map::new();

    for (field, key) in ENV_FIELDS {
        // Geheimnisse gehen nie zurück – nur, ob eines hinterlegt ist.
        if field.ends_with("_secret") {
            fields.insert(
                format!("{field}_set"),
                serde_json::Value::Bool(crate::envfile::is_set(values, key)),
            );
        } else {
            fields.insert(
                field.to_string(),
                serde_json::Value::String(crate::envfile::get(values, key).to_string()),
            );
        }
    }

    let mock = match crate::envfile::get(values, "NINJAONE_MOCK")
        .to_ascii_lowercase()
        .as_str()
    {
        "1" | "true" | "yes" => true,
        "0" | "false" | "no" => false,
        // Ohne Angabe entscheidet das Vorhandensein der Zugangsdaten.
        _ => {
            !crate::envfile::is_set(values, "NINJAONE_CLIENT_ID")
                || !crate::envfile::is_set(values, "NINJAONE_CLIENT_SECRET")
        }
    };

    fields.insert("mock".to_string(), serde_json::Value::Bool(mock));
    // Vollen Pfad zeigen: In der Oberfläche hilft „.env" niemandem weiter.
    let path = crate::envfile::path();
    let absolute = std::fs::canonicalize(&path).unwrap_or_else(|_| {
        std::env::current_dir()
            .map(|dir| dir.join(&path))
            .unwrap_or(path)
    });
    fields.insert(
        "file".to_string(),
        // Windows stellt `canonicalize` die erweiterte Pfadform voran – für
        // die Anzeige ist das nur Rauschen.
        serde_json::Value::String(
            absolute
                .display()
                .to_string()
                .trim_start_matches(r"\\?\")
                .to_string(),
        ),
    );

    serde_json::Value::Object(fields)
}

pub async fn get_env_settings(user: CurrentUser) -> ApiResult<Json<serde_json::Value>> {
    if !user.role.is_admin() {
        return Err(ApiError::Forbidden(
            "Nur Administratoren dürfen die Zugänge sehen.".to_string(),
        ));
    }

    Ok(Json(env_payload(&crate::envfile::read())))
}

#[derive(Debug, serde::Deserialize)]
pub struct EnvSettingsRequest {
    pub mock: bool,
    pub base_url: String,
    pub client_id: String,
    /// Leer lassen heißt: hinterlegtes Geheimnis behalten.
    #[serde(default)]
    pub client_secret: Option<String>,
    pub scope: String,
    #[serde(default)]
    pub user_client_id: String,
    #[serde(default)]
    pub user_client_secret: Option<String>,
    #[serde(default)]
    pub redirect_uri: String,
}

/// Baut aus den Eingaben eine NinjaOne-Konfiguration. `mock` steht bewusst auf
/// `false`: Eine Prüfung soll die echten Zugangsdaten testen, auch wenn die
/// App noch im Mock-Modus läuft.
fn candidate_config(
    payload: &EnvSettingsRequest,
    current: &std::collections::BTreeMap<String, String>,
) -> crate::config::NinjaConfig {
    let secret_from_form = |value: &Option<String>, key: &str| {
        value
            .clone()
            .filter(|text| !text.is_empty())
            .unwrap_or_else(|| crate::envfile::get(current, key).to_string())
    };

    crate::config::NinjaConfig {
        base_url: payload.base_url.trim().to_string(),
        client_id: payload.client_id.trim().to_string(),
        client_secret: secret_from_form(&payload.client_secret, "NINJAONE_CLIENT_SECRET"),
        scope: payload.scope.trim().to_string(),
        mock: false,
        user_client_id: payload.user_client_id.trim().to_string(),
        user_client_secret: secret_from_form(
            &payload.user_client_secret,
            "NINJAONE_USER_CLIENT_SECRET",
        ),
        redirect_uri: payload.redirect_uri.trim().to_string(),
    }
}

/// Prüft die Zugänge gegen NinjaOne, ohne etwas zu speichern.
async fn probe_ninja(config: &crate::config::NinjaConfig) -> Result<String, String> {
    if config.base_url.is_empty() {
        return Err("Server-Adresse fehlt.".to_string());
    }
    if config.client_id.is_empty() || config.client_secret.is_empty() {
        return Err("Client-ID und Client-Secret werden gebraucht.".to_string());
    }

    let client = crate::ninjaone::NinjaOneClient::new(config.clone());
    match client.list_organizations().await {
        Ok(organizations) => Ok(format!(
            "Verbindung steht. {} Organisationen sichtbar.",
            organizations.len()
        )),
        Err(error) => Err(format!("{error}")),
    }
}

pub async fn test_env_settings(
    user: CurrentUser,
    Json(payload): Json<EnvSettingsRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    if !user.role.is_admin() {
        return Err(ApiError::Forbidden(
            "Nur Administratoren dürfen die Zugänge prüfen.".to_string(),
        ));
    }

    let current = crate::envfile::read();
    let config = candidate_config(&payload, &current);

    Ok(Json(match probe_ninja(&config).await {
        Ok(report) => serde_json::json!({ "ok": true, "report": report }),
        Err(error) => serde_json::json!({ "ok": false, "report": error }),
    }))
}

pub async fn update_env_settings(
    State(state): State<SharedState>,
    user: CurrentUser,
    Json(payload): Json<EnvSettingsRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    if !user.role.is_admin() {
        return Err(ApiError::Forbidden(
            "Nur Administratoren dürfen die Zugänge ändern.".to_string(),
        ));
    }

    let current = crate::envfile::read();
    let config = candidate_config(&payload, &current);

    if config.base_url.is_empty() {
        return Err(ApiError::BadRequest("Server-Adresse fehlt.".to_string()));
    }

    // Im Live-Modus müssen die Zugangsdaten vollständig sein, sonst startet
    // die App mit einem Konto, das nichts darf – und der Fehler fällt erst
    // beim ersten Abruf auf.
    if !payload.mock && (config.client_id.is_empty() || config.client_secret.is_empty()) {
        return Err(ApiError::BadRequest(
            "Für den Live-Modus werden Client-ID und Client-Secret gebraucht. \
             Alternativ den Mock-Modus einschalten."
                .to_string(),
        ));
    }

    // Erst prüfen, dann speichern: Falsche Zugänge sollen die laufende
    // Verbindung nicht kaputt machen.
    let report = if payload.mock {
        "Mock-Modus – keine Verbindung geprüft.".to_string()
    } else {
        probe_ninja(&config).await.map_err(|error| {
            ApiError::BadRequest(format!(
                "Verbindung fehlgeschlagen – nichts gespeichert: {error}"
            ))
        })?
    };

    let mut updates = std::collections::BTreeMap::new();
    let put = |map: &mut std::collections::BTreeMap<String, Option<String>>, key: &str, value: String| {
        map.insert(key.to_string(), Some(value));
    };

    put(
        &mut updates,
        "NINJAONE_MOCK",
        if payload.mock { "true" } else { "false" }.to_string(),
    );
    put(&mut updates, "NINJAONE_BASE_URL", config.base_url.clone());
    put(&mut updates, "NINJAONE_CLIENT_ID", config.client_id.clone());
    put(&mut updates, "NINJAONE_SCOPE", config.scope.clone());
    put(
        &mut updates,
        "NINJAONE_USER_CLIENT_ID",
        config.user_client_id.clone(),
    );
    put(
        &mut updates,
        "NINJAONE_REDIRECT_URI",
        config.redirect_uri.clone(),
    );

    // Geheimnisse nur schreiben, wenn eines eingetippt wurde.
    updates.insert(
        "NINJAONE_CLIENT_SECRET".to_string(),
        payload.client_secret.clone().filter(|value| !value.is_empty()),
    );
    updates.insert(
        "NINJAONE_USER_CLIENT_SECRET".to_string(),
        payload
            .user_client_secret
            .clone()
            .filter(|value| !value.is_empty()),
    );

    let path = crate::envfile::write(&updates)
        .map_err(|error| ApiError::BadRequest(format!("Datei nicht schreibbar: {error}")))?;

    audit(
        &state.pool,
        user.actor(),
        "NinjaOne-Zugang geändert",
        Some("app_settings"),
        None,
        &format!(
            "{} · {} · {}",
            if payload.mock { "Mock-Modus" } else { "Live" },
            config.base_url,
            path.display()
        ),
    )
    .await?;

    let values = crate::envfile::read();

    Ok(Json(serde_json::json!({
        "ok": true,
        "report": report,
        "restart_required": true,
        "settings": env_payload(&values),
    })))
}

pub async fn get_billing_settings(
    State(state): State<SharedState>,
) -> ApiResult<Json<crate::settings::BillingSettings>> {
    Ok(Json(crate::settings::billing(&state.pool).await))
}

/// Ändert die Einstellungen. Nur Administratoren – hier hängen alle Beträge dran.
pub async fn update_billing_settings(
    State(state): State<SharedState>,
    user: CurrentUser,
    Json(payload): Json<crate::settings::UpdateSettingsRequest>,
) -> ApiResult<Json<crate::settings::BillingSettings>> {
    if !user.role.is_admin() {
        return Err(ApiError::Forbidden(
            "Nur Administratoren dürfen Stundensätze ändern.".to_string(),
        ));
    }

    let updated = crate::settings::update(&state.pool, &payload)
        .await
        .map_err(ApiError::BadRequest)?;

    audit(
        &state.pool,
        user.actor(),
        "Abrechnungseinstellungen geändert",
        Some("settings"),
        None,
        &format!(
            "Stundensatz {:.2} EUR netto, USt {:.1} %",
            updated.default_hourly_rate, updated.vat_rate
        ),
    )
    .await?;

    Ok(Json(updated))
}

/// Holt Patch- und Antivirusstand neu – ohne den kompletten Inventarabgleich.
pub async fn sync_monitoring(
    State(state): State<SharedState>,
    user: CurrentUser,
) -> ApiResult<Json<crate::monitoring::MonitoringSummary>> {
    let summary = crate::monitoring::refresh(&state.pool, &state.ninja)
        .await
        .map_err(|error| ApiError::Upstream(error.to_string()))?;

    audit(
        &state.pool,
        user.actor(),
        "Patch- und Antivirusstand aktualisiert",
        Some("integration"),
        None,
        &summary.message,
    )
    .await?;

    Ok(Json(summary))
}

pub async fn sync_ninjaone(
    State(state): State<SharedState>,
    user: CurrentUser,
) -> ApiResult<Json<SyncResult>> {
    let organizations = state
        .ninja
        .list_organizations()
        .await
        .map_err(|error| ApiError::Upstream(error.to_string()))?;

    let live = !state.ninja.is_mock();
    let now = now_iso();
    let mut devices = 0usize;
    let mut created_customers = 0usize;
    let mut updated_customers = 0usize;

    for organization in &organizations {
        if live {
            let short_code = unique_short_code(&state.pool, &organization.name, organization.id).await?;
            let customer_id = format!("ninja-{}", organization.id);
            let exists: Option<String> =
                sqlx::query_scalar("SELECT id FROM customers WHERE id = $1")
                    .bind(&customer_id)
                    .fetch_optional(&state.pool)
                    .await?;

            sqlx::query(
                "INSERT INTO customers
                 (id, name, short_code, ninja_org_id, contact_name, contact_email, phone, notes, active, created_at)
                 VALUES ($1, $2, $3, $4, NULL, NULL, NULL, $5, true, $6)
                 ON CONFLICT(id) DO UPDATE SET
                     name = excluded.name,
                     short_code = excluded.short_code,
                     ninja_org_id = excluded.ninja_org_id,
                     notes = excluded.notes,
                     active = true",
            )
            .bind(&customer_id)
            .bind(&organization.name)
            .bind(&short_code)
            .bind(organization.id)
            .bind(organization.description.as_deref())
            .bind(&now)
            .execute(&state.pool)
            .await?;

            if exists.is_some() {
                updated_customers += 1;
            } else {
                created_customers += 1;
            }
        }

        let organization_devices = state
            .ninja
            .list_devices(organization.id)
            .await
            .map_err(|error| ApiError::Upstream(error.to_string()))?;
        devices += organization_devices.len();

        if !live {
            continue;
        }

        for device in &organization_devices {
            let hostname = device
                .system_name
                .clone()
                .or_else(|| device.dns_name.clone())
                .unwrap_or_else(|| format!("Gerät {}", device.id));
            let (os, device_type) = map_node_class(device.node_class.as_deref());
            let last_seen = device
                .last_contact
                .and_then(|seconds| chrono::DateTime::from_timestamp(seconds as i64, 0))
                .map(|time| time.to_rfc3339_opts(chrono::SecondsFormat::Secs, true));
            let online = device
                .online
                .or_else(|| device.offline.map(|offline| !offline))
                .unwrap_or(true);

            sqlx::query(
                "INSERT INTO devices
                 (id, customer_id, ninja_device_id, hostname, os, device_type, last_seen, patch_status, antivirus_status, online)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, 'unknown', 'unknown', $8)
                 ON CONFLICT(id) DO UPDATE SET
                     customer_id = excluded.customer_id,
                     hostname = excluded.hostname,
                     os = excluded.os,
                     device_type = excluded.device_type,
                     last_seen = excluded.last_seen,
                     online = excluded.online",
            )
            .bind(format!("ninja-dev-{}", device.id))
            .bind(format!("ninja-{}", organization.id))
            .bind(device.id)
            .bind(&hostname)
            .bind(os)
            .bind(device_type)
            .bind(last_seen)
            .bind(online)
            .execute(&state.pool)
            .await?;
        }
    }

    let mut deactivated_demo = 0usize;
    if live {
        deactivated_demo = sqlx::query(
            "UPDATE customers SET active = false WHERE active = true AND id LIKE 'cust-%'",
        )
        .execute(&state.pool)
        .await?
        .rows_affected() as usize;

        if deactivated_demo > 0 {
            sqlx::query("DELETE FROM audit_events WHERE id LIKE 'audit-%'")
                .execute(&state.pool)
                .await?;
        }

        sqlx::query(
            "DELETE FROM audit_events WHERE id IN ('audit-1','audit-2','audit-3','audit-4','audit-5')",
        )
        .execute(&state.pool)
        .await?;

        let synced_ids: Vec<String> = organizations
            .iter()
            .map(|organization| format!("ninja-{}", organization.id))
            .collect();
        let stale: Vec<String> = sqlx::query_scalar(
            "SELECT id FROM customers WHERE active = true AND id LIKE 'ninja-%'",
        )
        .fetch_all(&state.pool)
        .await?;

        for customer_id in stale {
            if !synced_ids.contains(&customer_id) {
                sqlx::query("UPDATE customers SET active = false WHERE id = $1")
                    .bind(&customer_id)
                    .execute(&state.pool)
                    .await?;
            }
        }
    }

    // Patch- und Antivirusstand gehören zum Inventar dazu – ein Klick soll
    // alles aktualisieren. Ein Fehler dort macht den Geräteabgleich nicht kaputt.
    let mut monitoring_note = String::new();
    if live {
        match crate::monitoring::refresh(&state.pool, &state.ninja).await {
            Ok(summary) => monitoring_note = format!(" {}", summary.message),
            Err(error) => {
                tracing::warn!(%error, "Patch- und Antivirusstand nicht aktualisiert");
                monitoring_note = format!(" Patch-/Antivirusstand nicht aktualisiert: {error}");
            }
        }
    }

    let last_sync = now_iso();

    sqlx::query(
        "INSERT INTO app_settings (key, value) VALUES ('ninjaone.last_sync', $1)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
    )
    .bind(&last_sync)
    .execute(&state.pool)
    .await?;

    let mode = if state.ninja.is_mock() { "mock" } else { "live" };
    let message = if live {
        format!(
            "{} Organisationen und {} Geräte übernommen ({} neu, {} aktualisiert).{}",
            organizations.len(),
            devices,
            created_customers,
            updated_customers,
            monitoring_note
        )
    } else {
        format!(
            "{} Organisationen und {} Geräte abgeglichen (Mock-Modus).",
            organizations.len(),
            devices
        )
    };

    audit(
        &state.pool,
        user.actor(),
        "NinjaOne synchronisiert",
        Some("integration"),
        None,
        &message,
    )
    .await?;

    Ok(Json(SyncResult {
        mode: mode.to_string(),
        organizations: organizations.len(),
        devices,
        created_customers,
        updated_customers,
        deactivated_demo,
        last_sync,
        message,
    }))
}

fn map_node_class(node_class: Option<&str>) -> (Option<String>, Option<String>) {
    match node_class.unwrap_or_default() {
        "WINDOWS_SERVER" => (Some("Windows Server".to_string()), Some("server".to_string())),
        "WINDOWS_WORKSTATION" => (Some("Windows".to_string()), Some("workstation".to_string())),
        "MAC" => (Some("macOS".to_string()), Some("workstation".to_string())),
        "LINUX_SERVER" => (Some("Linux".to_string()), Some("server".to_string())),
        "LINUX_WORKSTATION" => (Some("Linux".to_string()), Some("workstation".to_string())),
        "NMS_SERVER" => (Some("Netzwerkgerät".to_string()), Some("appliance".to_string())),
        "VMWARE_VM_HOST" | "VMWARE_VM_GUEST" => (Some("VMware".to_string()), Some("server".to_string())),
        "" => (None, None),
        other => (Some(other.replace('_', " ")), Some("workstation".to_string())),
    }
}

async fn unique_short_code(pool: &PgPool, name: &str, org_id: i64) -> ApiResult<String> {
    let mut code: String = name
        .split(|character: char| character.is_whitespace() || character == '-' || character == '&')
        .filter(|word| !word.is_empty())
        .take(2)
        .map(|word| word.chars().next().unwrap_or('K').to_uppercase().to_string())
        .collect();

    if code.len() < 2 {
        code = name
            .chars()
            .filter(|character| character.is_alphanumeric())
            .take(2)
            .collect::<String>()
            .to_uppercase();
    }

    if code.is_empty() {
        code = format!("K{org_id}");
    }

    let taken: Option<String> = sqlx::query_scalar(
        "SELECT id FROM customers WHERE short_code = $1 AND id != $2 LIMIT 1",
    )
    .bind(&code)
    .bind(format!("ninja-{org_id}"))
    .fetch_optional(pool)
    .await?;

    if taken.is_some() {
        code = format!("{code}{}", org_id % 100);
    }

    Ok(code)
}

pub async fn integrations(
    State(state): State<SharedState>,
) -> ApiResult<Json<IntegrationStatus>> {
    Ok(Json(integration_status(&state).await?))
}

async fn integration_status(state: &SharedState) -> ApiResult<IntegrationStatus> {
    let last_sync: Option<String> = sqlx::query_scalar(
        "SELECT value FROM app_settings WHERE key = 'ninjaone.last_sync'",
    )
    .fetch_optional(&state.pool)
    .await?;

    // Nur aktive Kunden zählen – stillgelegte Demo-Geräte verfälschen sonst das Bild.
    let monitoring = sqlx::query_as::<_, (Option<String>, i64, i64, i64)>(
        "SELECT
            MAX(d.patch_checked_at),
            COUNT(CASE WHEN d.patch_status IN ('pending', 'ok') THEN 1 END),
            COALESCE(SUM(d.patch_pending), 0)::bigint,
            COUNT(CASE WHEN d.antivirus_status = 'warning' THEN 1 END)
         FROM devices d
         JOIN customers c ON c.id = d.customer_id
         WHERE c.active = true",
    )
    .fetch_one(&state.pool)
    .await?;

    let open_patches = sqlx::query_as::<_, (i64, i64)>(
        "SELECT
            COALESCE(SUM(CASE WHEN p.patch_kind = 'os' THEN 1 ELSE 0 END), 0)::bigint,
            COALESCE(SUM(CASE WHEN p.patch_kind = 'software' THEN 1 ELSE 0 END), 0)::bigint
         FROM device_patches p
         JOIN devices d ON d.id = p.device_id
         JOIN customers c ON c.id = d.customer_id
         WHERE c.active = true AND p.status IN ('APPROVED', 'DELAYED', 'MANUAL')",
    )
    .fetch_one(&state.pool)
    .await?;

    Ok(IntegrationStatus {
        monitoring_checked_at: monitoring.0,
        monitoring_patch_devices: monitoring.1,
        monitoring_pending_os: open_patches.0,
        monitoring_pending_software: open_patches.1,
        monitoring_av_warnings: monitoring.3,
        user_app_configured: state.ninja.user_app_configured(),
        user_connected: state.ninja.has_user_context().await,
        ninjaone_mode: if state.ninja.is_mock() {
            "mock".to_string()
        } else {
            "live".to_string()
        },
        ninjaone_base_url: state.ninja.base_url().to_string(),
        last_sync,
        wp_agent_configured: state.config.wp_shared_secret != "change-me-in-production",
        webhook_slots: "1 Webhook pro Tenant".to_string(),
    })
}

pub async fn list_audit(State(state): State<SharedState>) -> ApiResult<Json<Vec<AuditEvent>>> {
    let events =
        sqlx::query_as::<_, AuditEvent>("SELECT * FROM audit_events ORDER BY created_at DESC LIMIT 30")
            .fetch_all(&state.pool)
            .await?;

    Ok(Json(events))
}

async fn fetch_task(pool: &PgPool, id: &str) -> ApiResult<Task> {
    sqlx::query_as::<_, Task>("SELECT * FROM tasks WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Aufgabe {id} nicht gefunden")))
}
