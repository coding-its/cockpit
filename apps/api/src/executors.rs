use std::time::{Duration, Instant};

use anyhow::{bail, Result};
use sqlx::PgPool;

use crate::models::Task;
use crate::state::SharedState;
use crate::ninjaone::DeviceActivity;
use crate::wordpress::{WpAgent, WpAgentConfig};
use crate::util::{audit, now_iso};

const ACTIVITY_WAIT_SECONDS: u64 = 45;

/// Art des Vorgangs – verhindert, dass ein Script den Abschluss eines
/// vorherigen Patch-Scans als eigenen Nachweis übernimmt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ActivityKind {
    Patch,
    Script,
}

impl ActivityKind {
    fn matches(&self, activity: &DeviceActivity) -> bool {
        let kind = activity.activity_type.as_deref().unwrap_or("");
        let code = activity.status_code.as_deref().unwrap_or("");

        match self {
            ActivityKind::Patch => kind.contains("PATCH") || code.contains("PATCH"),
            // Scripts laufen bei NinjaOne als ACTION bzw. ACTIONSET.
            ActivityKind::Script => {
                let is_action = kind.starts_with("ACTION") || kind.contains("SCRIPT");
                let is_patch = kind.contains("PATCH") || code.contains("PATCH");
                is_action && !is_patch
            }
        }
    }
}

/// Wartet auf die Abschluss-Aktivität eines Vorgangs (Patch oder Script).
/// NinjaOne führt asynchron aus; die Geräte-Historie ist der Beleg.
async fn wait_for_completion_activity(
    state: &SharedState,
    device_id: i64,
    submitted_at: f64,
    kind: ActivityKind,
) -> Option<DeviceActivity> {
    let deadline = Instant::now() + Duration::from_secs(ACTIVITY_WAIT_SECONDS);

    loop {
        match state.ninja.list_device_activities(device_id).await {
            Ok(activities) => {
                if let Some(found) = activities
                    .into_iter()
                    .find(|activity| is_completion(activity, submitted_at, kind))
                {
                    return Some(found);
                }
            }
            Err(error) => {
                tracing::warn!(?error, device_id, "Aktivitäten nicht lesbar");
            }
        }

        if Instant::now() >= deadline {
            return None;
        }

        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}

pub async fn execute_task(
    state: SharedState,
    task: Task,
    job_id: String,
    execution_id: String,
) {
    let started = Instant::now();

    let outcome = if state.ninja.is_mock() {
        simulate(&state, &task, &job_id).await
    } else {
        if let Err(error) =
            progress(&state.pool, &job_id, 25, "Anfrage an NinjaOne wird gesendet").await
        {
            tracing::warn!(?error, "Job-Fortschritt konnte nicht gespeichert werden");
        }
        run_live(&state, &task, &job_id, &execution_id).await
    };

    let duration_ms = started.elapsed().as_millis() as i64;

    match outcome {
        Ok((summary, before, after, detail)) => {
            if let Err(error) = complete(
                &state.pool,
                &task,
                &job_id,
                &execution_id,
                &summary,
                before.as_deref(),
                after.as_deref(),
                detail.as_deref(),
                duration_ms,
            )
            .await
            {
                tracing::error!(?error, task_id = %task.id, "Ergebnis konnte nicht gespeichert werden");
            }
        }
        Err(error) => {
            tracing::error!(?error, task_id = %task.id, "Aufgabe fehlgeschlagen");
            if let Err(write_error) = fail(
                &state.pool,
                &task,
                &job_id,
                &execution_id,
                &error.to_string(),
                duration_ms,
            )
            .await
            {
                tracing::error!(?write_error, task_id = %task.id, "Fehlerstatus konnte nicht gespeichert werden");
            }
        }
    }
}

async fn simulate(
    state: &SharedState,
    task: &Task,
    job_id: &str,
) -> Result<(String, Option<String>, Option<String>, Option<String>)> {
    let steps = steps_for(task);

    for (index, step) in steps.iter().enumerate() {
        let delay = 380 + ((task.position as u64 * 97 + index as u64 * 131) % 360);
        tokio::time::sleep(Duration::from_millis(delay)).await;
        let percent = (((index + 1) as f64 / steps.len() as f64) * 92.0) as i64;
        progress(&state.pool, job_id, percent.max(5), step).await?;
    }

    progress(&state.pool, job_id, 97, "Ergebnis wird dokumentiert").await?;
    let (summary, before, after) = mock_result(task);
    Ok((summary, before, after, None))
}

async fn run_live(
    state: &SharedState,
    task: &Task,
    job_id: &str,
    execution_id: &str,
) -> Result<(String, Option<String>, Option<String>, Option<String>)> {
    match task.kind.as_str() {
        "NINJA_PATCH" => {
            let device_id = ninja_device_id(state, task).await?;
            let (scan, software) = patch_mode(task);
            let submitted_at = chrono::Utc::now().timestamp() as f64;

            let outcome = if scan {
                state.ninja.scan_patches(device_id, software).await?
            } else {
                state.ninja.apply_patches(device_id, software).await?
            };

            if let Err(error) = progress(
                &state.pool,
                job_id,
                60,
                "NinjaOne führt aus – Patch-Läufe brauchen meist ein bis drei Minuten",
            )
            .await
            {
                tracing::warn!(?error, "Fortschritt konnte nicht gespeichert werden");
            }

            // Der Nachweis liegt in der Geräte-Historie, nicht in /jobs.
            let activity =
                wait_for_completion_activity(state, device_id, submitted_at, ActivityKind::Patch)
                    .await;

            let endpoint = format!(
                "patch/{}/{}",
                if software { "software" } else { "os" },
                if scan { "scan" } else { "apply" }
            );

            let detail = serde_json::json!({
                "endpoint": endpoint,
                "device_id": device_id,
                "http_status": outcome.status,
                "body": outcome.body,
                "activity": activity,
            })
            .to_string();

            let label = patch_label(scan, software);

            let summary = match &activity {
                Some(found) => format!(
                    "{} abgeschlossen laut NinjaOne-Aktivität #{}: {}",
                    label,
                    found.id,
                    activity_text(found)
                ),
                None => {
                    spawn_evidence_follow_up(
                        state.clone(),
                        task.id.clone(),
                        execution_id.to_string(),
                        device_id,
                        submitted_at,
                        label.to_string(),
                        ActivityKind::Patch,
                    );
                    format!(
                        "{} läuft in NinjaOne (HTTP {}). Der Abschluss wird automatisch nachgetragen, sobald das Gerät meldet.",
                        label, outcome.status
                    )
                }
            };

            // NinjaOne meldet einen gescheiterten Lauf als abgeschlossene
            // Aktivität. Ohne diese Prüfung stünde „OK", obwohl nichts passiert ist.
            if let Some(found) = &activity {
                if activity_failed(found) {
                    // Beleg trotzdem ins Ausführungsprotokoll – die Antwort ist der Fehlertext.
                    if let Err(error) = sqlx::query(
                        "UPDATE task_executions SET response = $1 WHERE id = $2",
                    )
                    .bind(&detail)
                    .bind(execution_id)
                    .execute(&state.pool)
                    .await
                    {
                        tracing::warn!(?error, "Ausführungsprotokoll konnte nicht ergänzt werden");
                    }

                    bail!("{} fehlgeschlagen: {}", label, activity_text(found));
                }
            }

            Ok((summary, None, None, Some(detail)))
        }
        "NINJA_REBOOT" => {
            let device_id = ninja_device_id(state, task).await?;
            let forced = task.action.as_deref() == Some("reboot_forced");
            let params = action_params(task);
            let reason = params
                .get("reason")
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or("Wartung über Wartungscockpit")
                .to_string();

            let outcome = state
                .ninja
                .reboot_device(device_id, forced, &reason)
                .await?;

            let detail = serde_json::json!({
                "endpoint": if forced { "reboot/FORCED" } else { "reboot/NORMAL" },
                "device_id": device_id,
                "reason": reason,
                "http_status": outcome.status,
                "body": outcome.body,
            })
            .to_string();

            // Kein Warten auf eine Abschluss-Aktivität: das Gerät ist beim
            // Neustart offline und meldet erst danach wieder.
            let summary = format!(
                "Neustart angewiesen (HTTP {}){}. Das Gerät meldet sich nach dem Start wieder.",
                outcome.status,
                if forced { ", erzwungen" } else { "" }
            );

            Ok((summary, None, None, Some(detail)))
        }
        "NINJA_MAINTENANCE" => {
            let device_id = ninja_device_id(state, task).await?;

            if task.action.as_deref() == Some("maintenance_stop") {
                let outcome = state.ninja.cancel_maintenance(device_id).await?;
                forget_maintenance(state, task).await;

                let detail = serde_json::json!({
                    "endpoint": "maintenance",
                    "device_id": device_id,
                    "http_status": outcome.status,
                    "body": outcome.body,
                })
                .to_string();

                return Ok((
                    "Wartungsmodus beendet. Alarme greifen wieder.".to_string(),
                    None,
                    None,
                    Some(detail),
                ));
            }

            let params = action_params(task);
            let minutes = params
                .get("minutes")
                .and_then(serde_json::Value::as_i64)
                .unwrap_or(60)
                .clamp(1, 24 * 60);
            let features: Vec<String> = params
                .get("features")
                .and_then(|value| value.as_array())
                .map(|items| {
                    items
                        .iter()
                        .filter_map(|item| item.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();

            let outcome = state
                .ninja
                .start_maintenance(device_id, minutes, &features)
                .await?;

            remember_maintenance(state, device_id, minutes).await;

            let detail = serde_json::json!({
                "endpoint": "maintenance",
                "device_id": device_id,
                "minutes": minutes,
                "features": features,
                "http_status": outcome.status,
                "body": outcome.body,
            })
            .to_string();

            Ok((
                format!(
                    "Wartungsmodus für {} gesetzt. Alarme bleiben bis dahin unterdrückt.",
                    human_minutes(minutes)
                ),
                None,
                None,
                Some(detail),
            ))
        }
        "NINJA_SCRIPT" => {
            let device_id = ninja_device_id(state, task).await?;

            // Die Public API erlaubt kein Inline-Script. Wir nutzen eine
            // eingebaute Aktion ("DNS-Cache löschen") – harmlos und umkehrbar.
            let submitted_at = chrono::Utc::now().timestamp() as f64;
            let uid = resolve_action_uid(state, device_id).await?;
            let outcome = state
                .ninja
                .run_builtin_action(device_id, &uid, "SYSTEM")
                .await?;

            if let Err(error) = progress(
                &state.pool,
                job_id,
                60,
                "NinjaOne führt das Script aus",
            )
            .await
            {
                tracing::warn!(?error, "Fortschritt konnte nicht gespeichert werden");
            }

            let activity =
                wait_for_completion_activity(state, device_id, submitted_at, ActivityKind::Script)
                    .await;

            let detail = serde_json::json!({
                "endpoint": "script/run",
                "device_id": device_id,
                "run_as": "SYSTEM",
                "action_uid": uid,
                "http_status": outcome.status,
                "body": outcome.body,
                "activity": activity,
            })
            .to_string();

            let summary = match &activity {
                Some(found) => format!(
                    "Script ausgeführt laut NinjaOne-Aktivität #{}: {}",
                    found.id,
                    activity_text(found)
                ),
                None => {
                    spawn_evidence_follow_up(
                        state.clone(),
                        task.id.clone(),
                        execution_id.to_string(),
                        device_id,
                        submitted_at,
                        "Script-Ausführung".to_string(),
                        ActivityKind::Script,
                    );
                    format!(
                        "Script läuft in NinjaOne (HTTP {}). Der Abschluss wird automatisch nachgetragen.",
                        outcome.status
                    )
                }
            };

            Ok((summary, None, None, Some(detail)))
        }
        "WP_PLUGIN_UPDATE" | "WP_CORE_UPDATE" | "WP_HEALTH" => {
            run_wordpress(state, task).await
        }
        _ => Ok(("Manuell bestätigt.".to_string(), None, None, None)),
    }
}

/// Bevorzugt die inhaltliche Meldung (enthält oft die Script-Ausgabe),
/// sonst den Status. Lange Ausgaben werden gekürzt.
fn activity_text(activity: &DeviceActivity) -> String {
    let text = activity
        .message
        .as_deref()
        .map(str::trim)
        .filter(|message| !message.is_empty())
        .map(|message| message.split_whitespace().collect::<Vec<_>>().join(" "))
        .or_else(|| activity.status.clone())
        .or_else(|| activity.status_code.clone())
        .unwrap_or_else(|| "ohne Statusmeldung".to_string());

    if text.chars().count() > 220 {
        format!("{}…", text.chars().take(220).collect::<String>())
    } else {
        text
    }
}

/// NinjaOne meldet auch Fehlschläge als "COMPLETED" – der Text entscheidet.
fn activity_failed(activity: &DeviceActivity) -> bool {
    let code_failed = activity
        .status_code
        .as_deref()
        .map(|code| code.contains("FAILED"))
        .unwrap_or(false);

    // NinjaOne formuliert je nach Sprache und Vorgang unterschiedlich:
    // „konnte nicht abgeschlossen werden", „nicht genügend Speicherplatz",
    // „failed to complete". Diese Wendungen sind eindeutig negativ.
    let message_failed = activity
        .message
        .as_deref()
        .map(|message| {
            let upper = message.to_uppercase();
            upper.contains("FAILURE")
                || upper.contains("ERGEBNIS: FEHLER")
                || upper.contains("KONNTE NICHT")
                || upper.contains("NICHT ABGESCHLOSSEN")
                || upper.contains("NICHT AUSGEFÜHRT")
                || upper.contains("NICHT GENÜGEND")
                || upper.contains("FAILED TO")
                || upper.contains("COULD NOT BE COMPLETED")
        })
        .unwrap_or(false);

    code_failed || message_failed
}

fn is_completion(activity: &DeviceActivity, submitted_at: f64, kind: ActivityKind) -> bool {
    let recent = activity.activity_time.unwrap_or(0.0) >= submitted_at - 5.0;
    let completed = activity
        .status_code
        .as_deref()
        .map(|code| code.contains("COMPLETED") || code.contains("FAILED"))
        .unwrap_or(false);
    recent && completed && kind.matches(activity)
}

/// NinjaOne meldet den Abschluss oft erst nach Minuten. Statt den Job offen zu
/// lassen, trägt diese Aufgabe den Nachweis nachträglich ein.
fn spawn_evidence_follow_up(
    state: SharedState,
    task_id: String,
    execution_id: String,
    device_id: i64,
    submitted_at: f64,
    label: String,
    kind: ActivityKind,
) {
    tokio::spawn(async move {
        for _ in 0..180 {
            tokio::time::sleep(Duration::from_secs(5)).await;

            let Ok(activities) = state.ninja.list_device_activities(device_id).await else {
                continue;
            };

            let Some(found) = activities
                .iter()
                .find(|a| is_completion(a, submitted_at, kind))
            else {
                continue;
            };

            // Ein fehlgeschlagener Vorgang darf nicht als "erledigt" stehen bleiben.
            let failed = activity_failed(found);
            let summary = format!(
                "{} {} laut NinjaOne-Aktivität #{}: {}",
                label,
                if failed { "fehlgeschlagen" } else { "abgeschlossen" },
                found.id,
                activity_text(found)
            );
            let status = if failed { "failed" } else { "done" };

            if let Err(error) = sqlx::query(
                "UPDATE tasks SET result_summary = $1, status = $2 WHERE id = $3",
            )
            .bind(&summary)
            .bind(status)
            .bind(&task_id)
            .execute(&state.pool)
            .await
            {
                tracing::warn!(?error, task_id, "Nachweis konnte nicht gespeichert werden");
                return;
            }

            let detail = serde_json::json!({
                "device_id": device_id,
                "activity_id": found.id,
                "status_code": found.status_code,
                "status": found.status,
                "nachgetragen": true,
            })
            .to_string();

            if let Err(error) = sqlx::query(
                "UPDATE task_executions SET response = $1 WHERE id = $2",
            )
            .bind(&detail)
            .bind(&execution_id)
            .execute(&state.pool)
            .await
            {
                tracing::warn!(?error, "Ausführungsprotokoll konnte nicht ergänzt werden");
            }

            let _ = audit(
                &state.pool,
                "NinjaOne",
                "Abschluss nachgetragen",
                Some("task"),
                Some(&task_id),
                &summary,
            )
            .await;

            return;
        }
    });
}

/// Sucht die UID einer unschädlichen eingebauten Aktion auf dem Gerät.
/// Konfigurierbar über NINJAONE_POC_ACTION_NAME (Teilstring, Vorgabe "DNS").
async fn resolve_action_uid(state: &SharedState, device_id: i64) -> Result<String> {
    let needle = std::env::var("NINJAONE_POC_ACTION_NAME").unwrap_or_else(|_| "DNS".to_string());

    if state.ninja.is_mock() {
        return Ok("mock-action".to_string());
    }

    let options = state.ninja.device_scripting_options(device_id).await?;
    let empty = Vec::new();
    let entries = options
        .get("scripts")
        .and_then(|value| value.as_array())
        .unwrap_or(&empty);

    let found = entries.iter().find(|entry| {
        entry
            .get("type")
            .and_then(|value| value.as_str())
            .map(|kind| kind == "ACTION")
            .unwrap_or(false)
            && entry
                .get("name")
                .and_then(|value| value.as_str())
                .map(|name| name.to_uppercase().contains(&needle.to_uppercase()))
                .unwrap_or(false)
    });

    let uid = found
        .and_then(|entry| entry.get("uid"))
        .and_then(|value| value.as_str())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Keine eingebaute Aktion mit \"{needle}\" auf Gerät {device_id} gefunden"
            )
        })?;

    Ok(uid.to_string())
}


/// Führt eine WordPress-Aufgabe über den Mu-Plugin-Agenten aus.
async fn run_wordpress(
    state: &SharedState,
    task: &Task,
) -> Result<(String, Option<String>, Option<String>, Option<String>)> {
    let site_id = task
        .target_id
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("Aufgabe hat keine WordPress-Site als Ziel"))?;

    let site: Option<(String, Option<String>, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT url, agent_user, agent_password, agent_secret FROM sites WHERE id = $1",
    )
    .bind(site_id)
    .fetch_optional(&state.pool)
    .await?;

    let Some((url, user, password, secret)) = site else {
        bail!("Site {site_id} nicht gefunden");
    };

    let agent = WpAgent::new(WpAgentConfig {
        site_url: url,
        user: user.unwrap_or_default(),
        app_password: password.unwrap_or_default(),
        secret: secret.unwrap_or_default(),
    })?;

    match task.kind.as_str() {
        "WP_HEALTH" => {
            // 1) Agent liefert Versionen und Update-Zahlen.
            let data = agent.health().await?;
            update_site_from_health(state, site_id, &data).await?;

            let core_version = data
                .core
                .as_ref()
                .and_then(|core| core.version.clone())
                .unwrap_or_else(|| "unbekannt".to_string());
            let php_version = data
                .php
                .as_ref()
                .and_then(|php| php.version.clone());
            let plugin_updates = data
                .plugins
                .as_ref()
                .and_then(|plugins| plugins.updates_available)
                .unwrap_or(0);

            // 2) Erreichbarkeit prüft das Cockpit selbst – unabhängig vom Agenten.
            let reachable = check_reachable(&url_of(state, site_id).await?).await;

            let summary = match &reachable {
                Reachability::Ok { status } => format!(
                    "Health-Check OK: HTTP {status}. WordPress {core_version}, PHP {}, {plugin_updates} Plugin-Updates offen.",
                    php_version.unwrap_or_else(|| "?".to_string())
                ),
                Reachability::Fatal { status } => {
                    bail!("Health-Check fehlgeschlagen: HTTP {status} mit Fatal-Error im HTML")
                }
                Reachability::HttpError { status } => {
                    bail!("Health-Check fehlgeschlagen: HTTP {status}")
                }
                Reachability::Unreachable { error } => {
                    bail!("Health-Check fehlgeschlagen: {error}")
                }
            };

            let detail = serde_json::json!({
                "summary": summary.clone(),
                "agent": data,
                "reachability": reachable,
            })
            .to_string();

            Ok((summary, None, None, Some(detail)))
        }
        "WP_PLUGIN_UPDATE" => {
            // Erst Dry-Run: zeigt die geplanten Versionen ohne Änderung.
            let plan = agent.update_plugins(true, false).await?;
            let planned = plan.plugins.clone().unwrap_or_default();
            let actionable: Vec<_> = planned
                .iter()
                .filter(|item| !item.skipped.unwrap_or(false) && item.error.is_none())
                .collect();

            if actionable.is_empty() {
                let detail = serde_json::json!({ "dry_run": plan }).to_string();
                return Ok((
                    "Keine Plugin-Updates verfügbar.".to_string(),
                    None,
                    None,
                    Some(detail),
                ));
            }

            let before: Vec<String> = actionable
                .iter()
                .filter_map(|item| item.version_before.clone())
                .collect();
            let after: Vec<String> = actionable
                .iter()
                .filter_map(|item| item.version_after.clone())
                .collect();

            let confirmation = backup_confirmed(state, site_id).await?;
            if !confirmation {
                bail!(
                    "Backup nicht bestätigt. Vor einem Plugin-Update muss ein verifiziertes Backup vorliegen – der Agent sperrt die Ausführung sonst."
                );
            }

            let result = agent.update_plugins(false, true).await?;
            let health = result.health.clone();

            // Nach dem Update Versionen neu einlesen.
            if let Ok(data) = agent.health().await {
                update_site_from_health(state, site_id, &data).await?;
            }

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

            if !failed.is_empty() {
                bail!("Plugin-Update fehlgeschlagen – {}", failed.join("; "));
            }

            let summary = if updated.is_empty() {
                "Plugin-Updates ausgefuehrt, keine Versionsaenderung gemeldet.".to_string()
            } else {
                format!("Plugin-Updates abgeschlossen: {}", updated.join(", "))
            };

            let detail = serde_json::json!({
                "summary": summary.clone(),
                "dry_run": plan.plugins,
                "result": result.plugins,
                "health": health,
            })
            .to_string();

            Ok((
                summary,
                if before.is_empty() { None } else { Some(before.join(", ")) },
                if after.is_empty() { None } else { Some(after.join(", ")) },
                Some(detail),
            ))
        }
        "WP_CORE_UPDATE" => {
            let plan = agent.update_core(true, false).await?;

            if plan.current_version.as_deref() == plan.target_version.as_deref()
                || plan.target_version.is_none()
            {
                let detail = serde_json::json!({ "dry_run": plan }).to_string();
                return Ok((
                    format!(
                        "Kein Core-Update verfügbar. Installation ist auf {}.",
                        plan.current_version.unwrap_or_else(|| "unbekannt".to_string())
                    ),
                    None,
                    None,
                    Some(detail),
                ));
            }

            let confirmation = backup_confirmed(state, site_id).await?;
            if !confirmation {
                bail!(
                    "Backup nicht bestätigt. Vor einem Core-Update muss ein verifiziertes Backup vorliegen."
                );
            }

            let result = agent.update_core(false, true).await?;

            if let Ok(data) = agent.health().await {
                update_site_from_health(state, site_id, &data).await?;
            }

            let before = result.version_before.clone();
            let after = result.version_after.clone();

            if result.success == Some(false) {
                bail!(
                    "Core-Update fehlgeschlagen: {}",
                    result.error.unwrap_or_else(|| "unbekannter Fehler".to_string())
                );
            }

            let summary = format!(
                "Core aktualisiert: {} → {}. Health-Check ueber den Agenten durchgefuehrt.",
                before.clone().unwrap_or_else(|| "?".to_string()),
                after.clone().unwrap_or_else(|| "?".to_string())
            );
            let detail = serde_json::json!({
                "summary": summary.clone(),
                "dry_run": plan,
                "result": result,
            })
            .to_string();

            Ok((summary, before, after, Some(detail)))
        }
        other => bail!("Unbekannter WordPress-Aufgabentyp: {other}"),
    }
}

/// Backup-Bestätigung: 0 = nicht bestätigt, 1 = bestätigt.
async fn backup_confirmed(state: &SharedState, site_id: &str) -> Result<bool> {
    let value: Option<i64> =
        sqlx::query_scalar("SELECT backup_confirmed FROM sites WHERE id = $1")
            .bind(site_id)
            .fetch_optional(&state.pool)
            .await?;
    Ok(value.unwrap_or(0) == 1)
}

/// Schreibt Versionen und Update-Zahlen aus dem Agenten in die Site.
async fn update_site_from_health(
    state: &SharedState,
    site_id: &str,
    data: &crate::wordpress::WpHealthData,
) -> Result<()> {
    let wp_version = data
        .core
        .as_ref()
        .and_then(|core| core.version.clone());
    let php_version = data
        .php
        .as_ref()
        .and_then(|php| php.version.clone());
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
             plugin_updates = $3,
             core_update = $4,
             last_checked = $5
         WHERE id = $6",
    )
    .bind(wp_version)
    .bind(php_version)
    .bind(plugin_updates)
    .bind(core_update)
    .bind(now_iso())
    .bind(site_id)
    .execute(&state.pool)
    .await?;

    Ok(())
}

/// Ergebnis einer direkten Erreichbarkeitsprüfung.
#[derive(serde::Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Reachability {
    Ok { status: u16 },
    Fatal { status: u16 },
    HttpError { status: u16 },
    Unreachable { error: String },
}

/// Prüft die Startseite direkt und sucht nach Fatal-Error-Markern.
async fn check_reachable(url: &str) -> Reachability {
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .user_agent("Wartungscockpit/0.1")
        .build()
    {
        Ok(client) => client,
        Err(error) => {
            return Reachability::Unreachable {
                error: error.to_string(),
            }
        }
    };

    match client.get(url).send().await {
        Ok(response) => {
            let status = response.status().as_u16();
            let body = response.text().await.unwrap_or_default();
            let upper = body.to_uppercase();
            let fatal = upper.contains("FATAL ERROR")
                || upper.contains("PARSE ERROR")
                || upper.contains("THERE HAS BEEN A CRITICAL ERROR");

            if fatal {
                Reachability::Fatal { status }
            } else if (200..400).contains(&status) {
                Reachability::Ok { status }
            } else {
                Reachability::HttpError { status }
            }
        }
        Err(error) => Reachability::Unreachable {
            error: error.to_string(),
        },
    }
}

async fn url_of(state: &SharedState, site_id: &str) -> Result<String> {
    let url: Option<String> = sqlx::query_scalar("SELECT url FROM sites WHERE id = $1")
        .bind(site_id)
        .fetch_optional(&state.pool)
        .await?;

    url.ok_or_else(|| anyhow::anyhow!("Site {site_id} hat keine URL"))
}

async fn ninja_device_id(state: &SharedState, task: &Task) -> Result<i64> {
    let target_id = task
        .target_id
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("Aufgabe hat kein Zielgerät"))?;

    let device_id: Option<i64> =
        sqlx::query_scalar("SELECT ninja_device_id FROM devices WHERE id = $1")
            .bind(target_id)
            .fetch_one(&state.pool)
            .await?;

    device_id.ok_or_else(|| anyhow::anyhow!("Gerät {target_id} ist nicht mit NinjaOne verknüpft"))
}

/// Welcher Patch-Lauf gemeint ist. Neue Aufgaben tragen es in `action`,
/// ältere Aufträge haben nur den Titel – der bleibt als Rückfallebene.
fn patch_mode(task: &Task) -> (bool, bool) {
    match task.action.as_deref() {
        Some("patch_scan_os") => (true, false),
        Some("patch_apply_os") => (false, false),
        Some("patch_scan_software") => (true, true),
        Some("patch_apply_software") => (false, true),
        _ => {
            let scan = task.title.to_lowercase().contains("scan");
            let software = task.title.to_lowercase().contains("software");
            (scan, software)
        }
    }
}

fn patch_label(scan: bool, software: bool) -> &'static str {
    match (scan, software) {
        (true, false) => "OS-Patch-Scan",
        (false, false) => "OS-Update-Installation",
        (true, true) => "Software-Scan",
        (false, true) => "Software-Update-Installation",
    }
}

fn action_params(task: &Task) -> serde_json::Value {
    task.action_params
        .as_deref()
        .and_then(|raw| serde_json::from_str(raw).ok())
        .unwrap_or_else(|| serde_json::json!({}))
}

fn human_minutes(minutes: i64) -> String {
    if minutes >= 1440 && minutes % 1440 == 0 {
        let days = minutes / 1440;
        format!("{days} {}", if days == 1 { "Tag" } else { "Tage" })
    } else if minutes >= 60 && minutes % 60 == 0 {
        let hours = minutes / 60;
        format!("{hours} {}", if hours == 1 { "Stunde" } else { "Stunden" })
    } else {
        format!("{minutes} Minuten")
    }
}

/// Merkt sich das Ende des Wartungsfensters am Gerät. NinjaOne bietet keinen
/// Lese-Endpunkt dafür, also zeigen wir an, was wir selbst gesetzt haben.
async fn remember_maintenance(state: &SharedState, device_id: i64, minutes: i64) {
    let until = chrono::Utc::now() + chrono::Duration::minutes(minutes);

    if let Err(error) = sqlx::query(
        "UPDATE devices SET maintenance_until = $1 WHERE ninja_device_id = $2",
    )
    .bind(until.to_rfc3339())
    .bind(device_id)
    .execute(&state.pool)
    .await
    {
        tracing::warn!(?error, device_id, "Wartungsfenster konnte nicht vermerkt werden");
    }
}

async fn forget_maintenance(state: &SharedState, task: &Task) {
    let Some(target_id) = task.target_id.as_deref() else {
        return;
    };

    if let Err(error) =
        sqlx::query("UPDATE devices SET maintenance_until = NULL WHERE id = $1")
            .bind(target_id)
            .execute(&state.pool)
            .await
    {
        tracing::warn!(?error, target_id, "Wartungsvermerk konnte nicht gelöscht werden");
    }
}

fn steps_for(task: &Task) -> Vec<&'static str> {
    match task.kind.as_str() {
        "NINJA_PATCH" => {
            let (scan, software) = patch_mode(task);
            match (scan, software) {
                (true, false) => vec!["Gerät kontaktieren", "Patchbestand abrufen", "Ergebnis auswerten"],
                (true, true) => vec![
                    "Gerät kontaktieren",
                    "Installierte Programme prüfen",
                    "Softwarebestand abrufen",
                ],
                (false, true) => vec![
                    "Wartungsfenster prüfen",
                    "Software-Updates installieren",
                    "Ergebnis auswerten",
                ],
                (false, false) => vec![
                    "Wartungsfenster prüfen",
                    "Patches installieren",
                    "Neustartstatus prüfen",
                ],
            }
        }
        "NINJA_SCRIPT" => vec!["Script an Gerät senden", "Ausgabe prüfen"],
        "NINJA_REBOOT" => vec!["Neustart anweisen", "Gerät verabschiedet sich"],
        "NINJA_MAINTENANCE" => {
            if task.action.as_deref() == Some("maintenance_stop") {
                vec!["Wartungsfenster beenden", "Alarme wieder scharf"]
            } else {
                vec!["Wartungsfenster setzen", "Alarme unterdrücken"]
            }
        }
        "WP_PLUGIN_UPDATE" => vec![
            "Backup-Status prüfen",
            "Plugin-Updates laden",
            "Updates einspielen",
            "Health-Check ausführen",
        ],
        "WP_CORE_UPDATE" => vec![
            "Plugin-Kompatibilität prüfen",
            "Core-Update einspielen",
            "Health-Check ausführen",
        ],
        "WP_HEALTH" => vec!["HTTP-Status prüfen", "PHP-Fehlerlog prüfen"],
        _ => vec!["Arbeitsstand dokumentieren"],
    }
}

fn mock_result(task: &Task) -> (String, Option<String>, Option<String>) {
    let target = task.target_label.clone().unwrap_or_default();

    match task.kind.as_str() {
        "NINJA_PATCH" => {
            let (scan, software) = patch_mode(task);
            if scan {
                let what = if software { "Software" } else { "OS" };
                (
                    format!("{what}-Scan abgeschlossen: 2 Updates ausstehend, 0 kritische Patches."),
                    None,
                    None,
                )
            } else {
                (
                    "Patches installiert: 3 von 3. Neustart erforderlich, Praxisleitung informiert."
                        .to_string(),
                    Some("3 ausstehend".to_string()),
                    Some("0 ausstehend".to_string()),
                )
            }
        }
        "NINJA_REBOOT" => (
            "Neustart angewiesen. Das Gerät meldet sich nach dem Start wieder.".to_string(),
            None,
            None,
        ),
        "NINJA_MAINTENANCE" => {
            let ending = task.action.as_deref() == Some("maintenance_stop");
            (
                if ending {
                    "Wartungsmodus beendet. Alarme greifen wieder.".to_string()
                } else {
                    "Wartungsmodus gesetzt. Alarme bleiben bis zum Ende unterdrückt.".to_string()
                },
                None,
                None,
            )
        }
        "NINJA_SCRIPT" => (
            "Script ausgeführt: Exit-Code 0. Server meldet ausstehenden Neustart.".to_string(),
            None,
            None,
        ),
        "WP_PLUGIN_UPDATE" => {
            let count = if target.contains("kita") { 5 } else { 4 };
            (
                format!("{count} Plugins aktualisiert. Keine Fehler im PHP-Log."),
                Some(format!("{count} Updates ausstehend")),
                Some("0 Updates ausstehend".to_string()),
            )
        }
        "WP_CORE_UPDATE" => {
            let before = if target.contains("kita") { "6.4.3" } else { "6.5.5" };
            (
                format!("Core aktualisiert: {before} → 6.6.2. Startseite und Login geprüft."),
                Some(before.to_string()),
                Some("6.6.2".to_string()),
            )
        }
        "WP_HEALTH" => (
            "Health-Check OK: HTTP 200, Login und Formular funktionieren.".to_string(),
            None,
            None,
        ),
        _ => (
            "Manuell bestätigt und dokumentiert.".to_string(),
            None,
            None,
        ),
    }
}

async fn progress(pool: &PgPool, job_id: &str, percent: i64, message: &str) -> Result<()> {
    sqlx::query(
        "UPDATE jobs SET status = 'running', progress = $1, message = $2, updated_at = $3 WHERE id = $4",
    )
    .bind(percent)
    .bind(message)
    .bind(now_iso())
    .bind(job_id)
    .execute(pool)
    .await?;

    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn complete(
    pool: &PgPool,
    task: &Task,
    job_id: &str,
    execution_id: &str,
    summary: &str,
    before: Option<&str>,
    after: Option<&str>,
    detail: Option<&str>,
    duration_ms: i64,
) -> Result<()> {
    let now = now_iso();

    sqlx::query(
        "UPDATE tasks SET status = 'done', result_summary = $1, finished_at = $2 WHERE id = $3",
    )
    .bind(summary)
    .bind(&now)
    .bind(&task.id)
    .execute(pool)
    .await?;

    sqlx::query(
        "UPDATE jobs SET status = 'succeeded', progress = 100, message = $1, updated_at = $2 WHERE id = $3",
    )
    .bind(summary)
    .bind(&now)
    .bind(job_id)
    .execute(pool)
    .await?;

    sqlx::query(
        "UPDATE task_executions SET status = 'succeeded', response = $1, duration_ms = $2, before_version = $3, after_version = $4 WHERE id = $5",
    )
    .bind(detail.unwrap_or(summary))
    .bind(duration_ms)
    .bind(before)
    .bind(after)
    .bind(execution_id)
    .execute(pool)
    .await?;

    let open_tasks: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM tasks WHERE run_id = $1 AND status NOT IN ('done', 'skipped')",
    )
    .bind(&task.run_id)
    .fetch_one(pool)
    .await?;

    if open_tasks == 0 {
        sqlx::query(
            "UPDATE runs SET status = 'done', completed_at = $1 WHERE id = $2",
        )
        .bind(&now)
        .bind(&task.run_id)
        .execute(pool)
        .await?;
    }

    audit(
        pool,
        "Max Mustermann",
        "Aufgabe ausgeführt",
        Some("task"),
        Some(&task.id),
        summary,
    )
    .await?;

    Ok(())
}

async fn fail(
    pool: &PgPool,
    task: &Task,
    job_id: &str,
    execution_id: &str,
    error: &str,
    duration_ms: i64,
) -> Result<()> {
    let now = now_iso();

    sqlx::query(
        "UPDATE tasks SET status = 'failed', result_summary = $1, finished_at = $2 WHERE id = $3",
    )
    .bind(error)
    .bind(&now)
    .bind(&task.id)
    .execute(pool)
    .await?;

    // Fehlgeschlagene Aufgabe bedeutet Handlungsbedarf, nicht "läuft noch".
    sqlx::query("UPDATE runs SET status = 'blocked' WHERE id = $1 AND status = 'in_progress'")
        .bind(&task.run_id)
        .execute(pool)
        .await?;

    sqlx::query("UPDATE jobs SET status = 'failed', message = $1, updated_at = $2 WHERE id = $3")
        .bind(error)
        .bind(&now)
        .bind(job_id)
        .execute(pool)
        .await?;

    sqlx::query(
        "UPDATE task_executions SET status = 'failed', error = $1, duration_ms = $2 WHERE id = $3",
    )
    .bind(error)
    .bind(duration_ms)
    .bind(execution_id)
    .execute(pool)
    .await?;

    audit(
        pool,
        "Max Mustermann",
        "Aufgabe fehlgeschlagen",
        Some("task"),
        Some(&task.id),
        error,
    )
    .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn activity(message: &str, status_code: Option<&str>) -> DeviceActivity {
        DeviceActivity {
            id: 1,
            activity_type: Some("PATCH_MANAGEMENT_SCAN_COMPLETED".to_string()),
            status_code: status_code.map(str::to_string),
            status: Some("COMPLETED".to_string()),
            message: Some(message.to_string()),
            activity_time: None,
        }
    }

    /// NinjaOne schickt Fehlschläge als abgeschlossene Aktivität mit
    /// erklärendem Text. Genau dieser Fall stand vorher als „OK" in der Liste.
    #[test]
    fn german_failure_message_counts_as_failure() {
        let entry = activity(
            "Der OS-Patch-Scan konnte nicht abgeschlossen werden. Es ist nicht genügend Speicherplatz vorhanden.",
            Some("PATCH_SCAN_COMPLETED"),
        );
        assert!(activity_failed(&entry));
    }

    #[test]
    fn successful_scan_is_not_a_failure() {
        let entry = activity(
            "Patch scan completed: 3 patches pending.",
            Some("PATCH_SCAN_COMPLETED"),
        );
        assert!(!activity_failed(&entry));
    }
}
