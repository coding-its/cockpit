//! TeamViewer-Verbindungen.
//!
//! TeamViewer führt ein eigenes Verzeichnis von Kunden (Gruppen) und
//! protokolliert jede Fernwartungssitzung mit Beginn und Ende. Beides holt
//! dieses Modul über die Web-API und legt es lokal ab.
//!
//! Zwei Entscheidungen, die den Rest erklären:
//!
//! 1. **Kein eigener Kundenstamm.** Eine TeamViewer-Gruppe wird einem
//!    vorhandenen Kunden zugeordnet. Sonst stünde dieselbe Firma dreimal im
//!    System und die Auswertung pro Kunde zerfiele.
//! 2. **Keine automatische Abrechnung.** Verbindungszeit ist nicht
//!    Arbeitszeit – es gibt Sitzungen, die über Nacht offen blieben. Aus einer
//!    Verbindung wird erst ein Zeiteintrag, wenn jemand sie übernimmt.

use std::path::PathBuf;

use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use crate::util::now_iso;

const API: &str = "https://webapi.teamviewer.com/api/v1";

/// Grenze, ab der eine Sitzung als ungewöhnlich gilt (vier Stunden).
const UNUSUAL_SECONDS: i64 = 4 * 3600;

/// Zugangstoken: erst Umgebung, dann Datei.
///
/// Die Datei ist der praktische Weg – der Token wird im Management Console
/// einmal erzeugt und gilt lange. `data/` ist von Git ausgenommen.
pub fn token() -> Option<String> {
    if let Ok(value) = std::env::var("TEAMVIEWER_TOKEN") {
        let value = value.trim().to_string();
        if !value.is_empty() {
            return Some(value);
        }
    }

    token_paths()
        .into_iter()
        .find_map(|path| std::fs::read_to_string(path).ok())
        .map(|raw| raw.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn token_paths() -> Vec<PathBuf> {
    let mut paths = vec![PathBuf::from("data").join("teamviewer-token.txt")];

    if let Some(dir) = crate::dbconfig::DbConfig::app_data_dir() {
        paths.push(dir.join("teamviewer-token.txt"));
    }

    paths
}

pub fn configured() -> bool {
    token().is_some()
}

/// Woher der Token kommt – für die Anzeige in den Einstellungen.
///
/// `None`, wenn keiner hinterlegt ist. Sonst ein lesbarer Hinweis, damit
/// niemand rätselt, warum ein neu eingetragener Token nicht greift: Eine
/// Umgebungsvariable hat Vorrang vor der Datei.
pub fn token_source() -> Option<String> {
    if let Ok(value) = std::env::var("TEAMVIEWER_TOKEN") {
        if !value.trim().is_empty() {
            return Some("Umgebungsvariable TEAMVIEWER_TOKEN".to_string());
        }
    }

    token_paths()
        .into_iter()
        .find(|path| {
            std::fs::read_to_string(path)
                .map(|text| !text.trim().is_empty())
                .unwrap_or(false)
        })
        .map(|path| path.display().to_string())
}

/// Ablageort beim Speichern: vorhandene Datei überschreiben, sonst dort
/// anlegen, wo die Anwendung schreiben darf.
///
/// Unter `Program Files` darf niemand schreiben – deshalb `%APPDATA%` zuerst,
/// wenn noch keine Datei existiert.
fn target_path() -> PathBuf {
    if let Some(existing) = token_paths().into_iter().find(|path| path.exists()) {
        return existing;
    }

    crate::dbconfig::DbConfig::app_data_dir()
        .map(|dir| dir.join("teamviewer-token.txt"))
        .unwrap_or_else(|| PathBuf::from("data").join("teamviewer-token.txt"))
}

/// Legt den Token ab. Ein vorhandener wird überschrieben.
pub fn save_token(value: &str) -> Result<PathBuf, String> {
    let value = value.trim();
    if value.is_empty() {
        return Err("Der Token darf nicht leer sein.".to_string());
    }

    // Steuerzeichen deuten auf einen kopierten Zeilenumbruch oder eine
    // kaputte Zwischenablage – lieber melden als speichern.
    if value.chars().any(|c| c.is_control()) {
        return Err(
            "Der Token enthält ein Steuerzeichen. Bitte ohne Zeilenumbruch einfügen."
                .to_string(),
        );
    }

    let path = target_path();

    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)
            .map_err(|error| format!("Ordner {} nicht anlegbar: {error}", dir.display()))?;
    }

    std::fs::write(&path, value)
        .map_err(|error| format!("Datei {} nicht schreibbar: {error}", path.display()))?;

    Ok(path)
}

/// Entfernt den Token aus der Datei.
///
/// Eine Umgebungsvariable lässt sich hier nicht ändern – das wird gemeldet,
/// statt still wirkungslos zu bleiben.
pub fn clear_token() -> Result<(), String> {
    if let Ok(value) = std::env::var("TEAMVIEWER_TOKEN") {
        if !value.trim().is_empty() {
            return Err("Der Token kommt aus der Umgebungsvariable TEAMVIEWER_TOKEN. Bitte dort \
                        entfernen – die Datei hier hätte keine Wirkung."
                .to_string());
        }
    }

    for path in token_paths() {
        if path.exists() {
            std::fs::remove_file(&path)
                .map_err(|error| format!("{} nicht löschbar: {error}", path.display()))?;
        }
    }

    Ok(())
}

/// Prüft einen Token gegen die TeamViewer-API, ohne ihn zu speichern.
///
/// Bei fehlender Berechtigung nennt die Meldung den genauen Punkt im
/// Management Console – das ist der häufigste Fehler beim Anlegen.
pub async fn probe(value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() {
        return Err("Bitte einen Token eingeben.".to_string());
    }

    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let url = format!(
        "{API}/reports/connections?from_date={today}T00:00:00Z&to_date={today}T23:59:59Z"
    );

    let client = reqwest::Client::new();
    let response = client
        .get(&url)
        .bearer_auth(value)
        .send()
        .await
        .map_err(|error| format!("TeamViewer nicht erreichbar: {error}"))?;

    let status = response.status();

    if status.as_u16() == 401 {
        return Err("Der Token wird abgelehnt. Bitte Zeichen für Zeichen prüfen – er wird nur \
                    einmal angezeigt und lässt sich nicht nachlesen."
            .to_string());
    }

    if status.as_u16() == 403 {
        return Err("Der Token ist gültig, aber ihm fehlt die Berechtigung \
                    „Verbindungsprotokollierung → Verbindungsprotokolle lesen“."
            .to_string());
    }

    if !status.is_success() {
        return Err(format!("TeamViewer antwortet mit HTTP {}.", status.as_u16()));
    }

    let payload: ConnectionsResponse = response
        .json()
        .await
        .map_err(|error| format!("Antwort nicht lesbar: {error}"))?;

    let with_device = payload
        .records
        .iter()
        .filter(|record| record.devicename.is_some())
        .count();

    Ok(format!(
        "Zugang gültig · {} Verbindung(en) seit Mitternacht, davon {with_device} mit Gerätenamen.",
        payload.records.len()
    ))
}

#[derive(Debug, Deserialize)]
struct ConnectionsResponse {
    #[serde(default)]
    records: Vec<ConnectionRecord>,
}

#[derive(Debug, Deserialize)]
struct ConnectionRecord {
    id: String,
    #[serde(default)]
    username: Option<String>,
    #[serde(default)]
    deviceid: Option<String>,
    #[serde(default)]
    devicename: Option<String>,
    #[serde(default)]
    groupid: Option<String>,
    #[serde(default)]
    groupname: Option<String>,
    start_date: String,
    /// Fehlt bei Sitzungen, die beim Abruf noch laufen.
    #[serde(default)]
    end_date: Option<String>,
    #[serde(default)]
    fee: f64,
    #[serde(default)]
    notes: Option<String>,
}

/// TeamViewer liefert `2026-11-11T10:28:00Z`; ältere Sätze ohne Bindestriche
/// und Doppelpunkte (`20130116T194014Z`). Beides wird gelesen.
fn parse_time(raw: &str) -> Option<NaiveDateTime> {
    ["%Y-%m-%dT%H:%M:%SZ", "%Y%m%dT%H%M%SZ"]
        .iter()
        .find_map(|format| NaiveDateTime::parse_from_str(raw.trim(), format).ok())
}

fn shorten(value: &str, max: usize) -> String {
    let cleaned = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if cleaned.chars().count() <= max {
        return cleaned;
    }
    let mut out: String = cleaned.chars().take(max).collect();
    out.push('…');
    out
}

/// Ein Abruf-Ergebnis für die Oberfläche.
#[derive(Debug, Serialize)]
pub struct SyncReport {
    pub groups: usize,
    pub sessions: usize,
    pub message: String,
}

/// Holt die Verbindungen eines Zeitraums und legt sie ab.
///
/// `from` und `to` sind `YYYY-MM-DD`. TeamViewer arbeitet in 24-Stunden-
/// Fenstern; ein Monat in einem Aufruf ist zulässig und bleibt deutlich unter
/// den Seitengrenzen.
pub async fn sync(pool: &PgPool, from: &str, to: &str) -> Result<SyncReport, String> {
    let token = token().ok_or_else(|| {
        "Kein TeamViewer-Token hinterlegt. Erwartet wird der Token in \
         data/teamviewer-token.txt oder in der Umgebungsvariable TEAMVIEWER_TOKEN."
            .to_string()
    })?;

    let url =
        format!("{API}/reports/connections?from_date={from}T00:00:00Z&to_date={to}T23:59:59Z");

    let client = reqwest::Client::new();
    let response = client
        .get(&url)
        .bearer_auth(&token)
        .send()
        .await
        .map_err(|error| format!("TeamViewer nicht erreichbar: {error}"))?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        let hint = match status.as_u16() {
            401 => " Der Token ist ungültig oder abgelaufen.",
            403 => " Dem Token fehlt die Berechtigung in der Verbindungsprotokollierung.",
            _ => "",
        };
        return Err(format!(
            "TeamViewer antwortet mit HTTP {} ({}).{hint}",
            status.as_u16(),
            shorten(&body, 200)
        ));
    }

    // Text lesen und selbst auswerten: Dann nennt der Fehler die Stelle im
    // Datensatz, statt nur „nicht lesbar" zu sagen.
    let body = response
        .text()
        .await
        .map_err(|error| format!("Antwort nicht lesbar: {error}"))?;

    let payload: ConnectionsResponse = serde_json::from_str(&body)
        .map_err(|error| format!("Antwort nicht lesbar: {error}"))?;

    let now = now_iso();
    // Gruppen sind je Verbindung dabei; gezählt werden die verschiedenen.
    let mut seen_groups = std::collections::HashSet::new();
    let mut sessions = 0usize;

    for record in payload.records {
        let started = parse_time(&record.start_date);
        let ended = record.end_date.as_deref().and_then(parse_time);

        let Some(started) = started else {
            tracing::warn!(start = %record.start_date, "TeamViewer-Satz ohne lesbares Datum");
            continue;
        };

        let duration = ended
            .map(|end| (end - started).num_seconds().max(0))
            .unwrap_or(0);

        if let (Some(group_id), Some(group_name)) = (&record.groupid, &record.groupname) {
            sqlx::query(
                "INSERT INTO teamviewer_groups (group_id, name, last_seen, updated_at)
                 VALUES ($1, $2, $3, $3)
                 ON CONFLICT (group_id) DO UPDATE
                    SET name = EXCLUDED.name,
                        last_seen = EXCLUDED.last_seen,
                        updated_at = EXCLUDED.updated_at",
            )
            .bind(group_id)
            .bind(group_name)
            .bind(&now)
            .execute(pool)
            .await
            .map_err(|error| error.to_string())?;

            seen_groups.insert(group_id.clone());
        }

        // Gruppenlose Sitzungen (Ad-hoc-Kunden) kommen vor; sie bleiben ohne
        // Zuordnung sichtbar, statt stillschweigend zu verschwinden.
        sqlx::query(
            "INSERT INTO teamviewer_sessions
                (id, group_id, group_name, device_id, device_name, user_name,
                 started_at, ended_at, duration_seconds, fee, notes, synced_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
             ON CONFLICT (id) DO UPDATE
                SET group_id = EXCLUDED.group_id,
                    group_name = EXCLUDED.group_name,
                    device_id = EXCLUDED.device_id,
                    device_name = EXCLUDED.device_name,
                    user_name = EXCLUDED.user_name,
                    started_at = EXCLUDED.started_at,
                    ended_at = EXCLUDED.ended_at,
                    duration_seconds = EXCLUDED.duration_seconds,
                    fee = EXCLUDED.fee,
                    notes = EXCLUDED.notes,
                    synced_at = EXCLUDED.synced_at",
        )
        .bind(&record.id)
        .bind(&record.groupid)
        .bind(&record.groupname)
        .bind(&record.deviceid)
        .bind(&record.devicename)
        .bind(&record.username)
        .bind(started.format("%Y-%m-%dT%H:%M:%S").to_string())
        .bind(ended.map(|end| end.format("%Y-%m-%dT%H:%M:%S").to_string()))
        .bind(duration)
        .bind(record.fee)
        .bind(record.notes)
        .bind(&now)
        .execute(pool)
        .await
        .map_err(|error| error.to_string())?;

        sessions += 1;
    }

    let groups = seen_groups.len();

    Ok(SyncReport {
        groups,
        sessions,
        message: format!(
            "{sessions} Verbindungen und {groups} Gruppen im Zeitraum {from} bis {to} abgeglichen."
        ),
    })
}

// ---------------------------------------------------------------------------
// Sicht für die Oberfläche
// ---------------------------------------------------------------------------

/// Eine TeamViewer-Gruppe mit ihren Verbindungen im gewählten Monat.
#[derive(Debug, Serialize)]
pub struct GroupRow {
    pub group_id: String,
    pub name: String,
    pub customer_id: Option<String>,
    pub web_client_id: Option<String>,
    /// Zugeordneter Kunde oder Web-Kunde, wie er in der Leiste erscheint.
    pub assigned_to: Option<String>,
    pub account_code: Option<String>,
    pub sessions: i64,
    pub open_sessions: i64,
    pub seconds: i64,
    pub open_seconds: i64,
    pub fee: f64,
    /// Vorgeschlagener Kunde aus dem Namensvergleich – nicht gespeichert.
    pub suggested_id: Option<String>,
    pub suggested_name: Option<String>,
}

#[derive(Debug, sqlx::FromRow)]
struct RawGroupRow {
    group_id: String,
    name: String,
    customer_id: Option<String>,
    web_client_id: Option<String>,
    assigned_to: Option<String>,
    account_code: Option<String>,
    sessions: i64,
    open_sessions: i64,
    seconds: i64,
    open_seconds: i64,
    fee: f64,
}

/// Eine einzelne Verbindung.
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct SessionRow {
    pub id: String,
    pub group_id: Option<String>,
    pub group_name: Option<String>,
    pub device_name: Option<String>,
    pub user_name: Option<String>,
    pub started_at: String,
    pub ended_at: Option<String>,
    /// Verbindungsdauer laut TeamViewer – das Original.
    pub duration_seconds: i64,
    /// Von Hand gesetzte Dauer; `null` heißt: Original gilt.
    pub duration_override_seconds: Option<i64>,
    /// Dauer, die für die Übernahme gilt.
    pub effective_seconds: i64,
    pub fee: f64,
    /// Notiz aus TeamViewer.
    pub notes: Option<String>,
    /// Nachgearbeitete Notiz; `null` heißt: Original gilt.
    pub note_edit: Option<String>,
    /// Notiz, die für die Übernahme gilt.
    pub effective_note: Option<String>,
    pub imported_entry_id: Option<String>,
    pub ignored: bool,
    /// Übergroße Sitzung – meist ein vergessener Logout.
    pub unusual: bool,
}

/// Alle Gruppen mit Kennzahlen für einen Monat (`YYYY-MM`).
///
/// Der Zuordnungsvorschlag entsteht in Rust: Ein Namensvergleich in SQL wäre
/// unlesbar, und der Vorschlag ist ohnehin nur ein Hinweis – entschieden wird
/// in der Oberfläche.
pub async fn groups(pool: &PgPool, month: &str) -> Result<Vec<GroupRow>, sqlx::Error> {
    let rows = sqlx::query_as::<_, RawGroupRow>(
        r#"
        SELECT
            g.group_id,
            g.name,
            g.customer_id,
            g.web_client_id,
            COALESCE(c.name, w.name) AS assigned_to,
            a.code AS account_code,
            (SELECT COUNT(*) FROM teamviewer_sessions s
              WHERE s.group_id = g.group_id AND substr(s.started_at, 1, 7) = $1)::bigint AS sessions,
            (SELECT COUNT(*) FROM teamviewer_sessions s
              WHERE s.group_id = g.group_id AND substr(s.started_at, 1, 7) = $1
                AND s.imported_entry_id IS NULL AND s.ignored = false)::bigint AS open_sessions,
            COALESCE((SELECT SUM(COALESCE(s.duration_override_seconds, s.duration_seconds))
                        FROM teamviewer_sessions s
              WHERE s.group_id = g.group_id AND substr(s.started_at, 1, 7) = $1), 0)::bigint AS seconds,
            COALESCE((SELECT SUM(COALESCE(s.duration_override_seconds, s.duration_seconds))
                        FROM teamviewer_sessions s
              WHERE s.group_id = g.group_id AND substr(s.started_at, 1, 7) = $1
                AND s.imported_entry_id IS NULL AND s.ignored = false), 0)::bigint AS open_seconds,
            COALESCE((SELECT SUM(s.fee) FROM teamviewer_sessions s
              WHERE s.group_id = g.group_id AND substr(s.started_at, 1, 7) = $1), 0)::double precision AS fee
        FROM teamviewer_groups g
        LEFT JOIN customers c ON c.id = g.customer_id
        LEFT JOIN web_clients w ON w.id = g.web_client_id
        LEFT JOIN accounts a ON a.id = COALESCE(c.account_id, w.account_id)
        WHERE g.ignored = false
        ORDER BY g.name
        "#,
    )
    .bind(month)
    .fetch_all(pool)
    .await?;

    let candidates = targets(pool).await?;

    Ok(rows
        .into_iter()
        .map(|row| {
            let suggestion = if row.customer_id.is_none() && row.web_client_id.is_none() {
                candidates
                    .iter()
                    .find(|(_, name)| matches(&normalize(&row.name), &normalize(name)))
                    .map(|(id, name)| (id.clone(), name.clone()))
            } else {
                None
            };

            GroupRow {
                group_id: row.group_id,
                name: row.name,
                customer_id: row.customer_id,
                web_client_id: row.web_client_id,
                assigned_to: row.assigned_to,
                account_code: row.account_code,
                sessions: row.sessions,
                open_sessions: row.open_sessions,
                seconds: row.seconds,
                open_seconds: row.open_seconds,
                fee: row.fee,
                suggested_id: suggestion.as_ref().map(|(id, _)| id.clone()),
                suggested_name: suggestion.map(|(_, name)| name),
            }
        })
        .collect())
}

/// Mögliche Ziele einer Zuordnung: Kunden und Web-Kunden.
async fn targets(pool: &PgPool) -> Result<Vec<(String, String)>, sqlx::Error> {
    sqlx::query_as(
        "SELECT id, name FROM customers WHERE active = true
         UNION ALL
         SELECT id, name FROM web_clients WHERE active = true
         ORDER BY name",
    )
    .fetch_all(pool)
    .await
}

/// Verbindungen eines Monats, wahlweise einer Gruppe.
pub async fn sessions(
    pool: &PgPool,
    month: &str,
    group_id: Option<&str>,
) -> Result<Vec<SessionRow>, sqlx::Error> {
    sqlx::query_as::<_, SessionRow>(
        r#"
        SELECT
            id, group_id, group_name, device_name, user_name,
            started_at, ended_at,
            duration_seconds,
            duration_override_seconds,
            COALESCE(duration_override_seconds, duration_seconds) AS effective_seconds,
            fee,
            notes,
            note_edit,
            COALESCE(NULLIF(note_edit, ''), notes) AS effective_note,
            imported_entry_id,
            ignored,
            (COALESCE(duration_override_seconds, duration_seconds) >= $3) AS unusual
        FROM teamviewer_sessions
        WHERE substr(started_at, 1, 7) = $1
          AND ($2 IS NULL OR group_id = $2)
        ORDER BY started_at DESC
        "#,
    )
    .bind(month)
    .bind(group_id)
    .bind(UNUSUAL_SECONDS)
    .fetch_all(pool)
    .await
}

/// Ordnet eine Gruppe einem Kunden oder Web-Kunden zu.
///
/// `None` in beiden Feldern löst die Zuordnung. Genau eines darf gesetzt sein.
pub async fn assign(
    pool: &PgPool,
    group_id: &str,
    customer_id: Option<&str>,
    web_client_id: Option<&str>,
) -> Result<(), String> {
    if customer_id.is_some() && web_client_id.is_some() {
        return Err("Bitte entweder Kunde oder Web-Kunde angeben, nicht beides.".to_string());
    }

    let exists: Option<String> =
        sqlx::query_scalar("SELECT group_id FROM teamviewer_groups WHERE group_id = $1")
            .bind(group_id)
            .fetch_optional(pool)
            .await
            .map_err(|error| error.to_string())?;

    if exists.is_none() {
        return Err(format!("TeamViewer-Gruppe {group_id} nicht gefunden"));
    }

    sqlx::query(
        "UPDATE teamviewer_groups
            SET customer_id = $1, web_client_id = $2, updated_at = $3
          WHERE group_id = $4",
    )
    .bind(customer_id)
    .bind(web_client_id)
    .bind(now_iso())
    .bind(group_id)
    .execute(pool)
    .await
    .map_err(|error| error.to_string())?;

    Ok(())
}

/// Blendet eine Gruppe aus oder holt sie zurück.
pub async fn set_ignored(pool: &PgPool, group_id: &str, ignored: bool) -> Result<(), String> {
    let result = sqlx::query(
        "UPDATE teamviewer_groups SET ignored = $1, updated_at = $2 WHERE group_id = $3",
    )
    .bind(ignored)
    .bind(now_iso())
    .bind(group_id)
    .execute(pool)
    .await
    .map_err(|error| error.to_string())?;

    if result.rows_affected() == 0 {
        return Err(format!("TeamViewer-Gruppe {group_id} nicht gefunden"));
    }

    Ok(())
}

/// Ändert eine einzelne Sitzung.
///
/// Alle Angaben sind optional und werden nur gesetzt, wenn sie übergeben
/// werden – so kann die Oberfläche eine Zeile ändern, ohne die übrigen zu
/// kennen.
///
/// * `note`: leerer Text setzt auf die TeamViewer-Notiz zurück (`NULL`).
/// * `duration_seconds`: `None` lässt die Dauer, `Some(0)` setzt sie auf die
///   Verbindungsdauer zurück, ein Wert > 0 überschreibt sie.
/// * `ignored`: blendet die Sitzung aus bzw. holt sie zurück.
///
/// Eine bereits übernommene Sitzung lässt sich nicht mehr ändern: Der
/// Zeiteintrag ist dann die Wahrheit, sonst zeigten Nachweis und Sitzung
/// unterschiedliche Zeiten.
pub async fn edit_session(
    pool: &PgPool,
    id: &str,
    note: Option<&str>,
    duration_seconds: Option<i64>,
    ignored: Option<bool>,
) -> Result<(), String> {
    let existing: Option<Option<String>> =
        sqlx::query_scalar("SELECT imported_entry_id FROM teamviewer_sessions WHERE id = $1")
            .bind(id)
            .fetch_optional(pool)
            .await
            .map_err(|error| error.to_string())?;

    let Some(imported) = existing else {
        return Err(format!("Verbindung {id} nicht gefunden"));
    };

    if imported.is_some() {
        return Err(
            "Diese Verbindung wurde bereits als Zeiteintrag übernommen. Die Zeit lässt sich              jetzt nur noch unter „Zeiten“ ändern."
                .to_string(),
        );
    }

    if let Some(note) = note {
        let trimmed = note.trim();
        let value = if trimmed.is_empty() {
            None
        } else {
            Some(if trimmed.chars().count() > 400 {
                trimmed.chars().take(400).collect::<String>()
            } else {
                trimmed.to_string()
            })
        };

        sqlx::query("UPDATE teamviewer_sessions SET note_edit = $1 WHERE id = $2")
            .bind(&value)
            .bind(id)
            .execute(pool)
            .await
            .map_err(|error| error.to_string())?;
    }

    if let Some(seconds) = duration_seconds {
        // 0 und negative Werte bedeuten: zurück auf die Verbindungsdauer.
        let value = if seconds <= 0 { None } else { Some(seconds) };

        if let Some(value) = value {
            // Absurd lange Korrekturen abfangen (mehr als 24 Stunden).
            if value > 24 * 3600 {
                return Err("Eine Dauer über 24 Stunden ist nicht plausibel.".to_string());
            }
        }

        sqlx::query("UPDATE teamviewer_sessions SET duration_override_seconds = $1 WHERE id = $2")
            .bind(value)
            .bind(id)
            .execute(pool)
            .await
            .map_err(|error| error.to_string())?;
    }

    if let Some(ignored) = ignored {
        sqlx::query("UPDATE teamviewer_sessions SET ignored = $1 WHERE id = $2")
            .bind(ignored)
            .bind(id)
            .execute(pool)
            .await
            .map_err(|error| error.to_string())?;
    }

    Ok(())
}

/// Ergebnis einer Übernahme.
#[derive(Debug, Serialize)]
pub struct ImportReport {
    pub created: u64,
    pub skipped: u64,
    pub message: String,
}

/// Macht aus den Verbindungen einer Gruppe Zeiteinträge.
///
/// Nur zugeordnete Gruppen, nur nicht übernommene Sitzungen, nur der gewählte
/// Monat. Die Dauer kommt aus den Zeitstempeln und bleibt damit sekundengenau –
/// dieselbe Grundlage wie bei allen anderen Zeiten.
pub async fn import_sessions(
    pool: &PgPool,
    group_id: &str,
    month: &str,
    billable: bool,
) -> Result<ImportReport, String> {
    let target: Option<(Option<String>, Option<String>, String)> = sqlx::query_as(
        "SELECT customer_id, web_client_id, name
         FROM teamviewer_groups WHERE group_id = $1",
    )
    .bind(group_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| error.to_string())?;

    let Some((customer_id, web_client_id, name)) = target else {
        return Err(format!("TeamViewer-Gruppe {group_id} nicht gefunden"));
    };

    if customer_id.is_none() && web_client_id.is_none() {
        return Err("Diese Gruppe ist noch keinem Kunden zugeordnet. Ohne Zuordnung weiß das \
                    Cockpit nicht, wem die Zeit gehören soll."
            .to_string());
    }

    // Korrigierte Dauer und nachgearbeitete Notiz haben Vorrang; ignorierte
    // Sitzungen (Duplikate, Fehlklicks) bleiben draußen.
    let open: Vec<(String, String, Option<String>, Option<String>, i64, Option<String>)> =
        sqlx::query_as(
            "SELECT id, started_at, ended_at, user_name,
                    COALESCE(duration_override_seconds, duration_seconds),
                    COALESCE(NULLIF(note_edit, ''), notes)
             FROM teamviewer_sessions
             WHERE group_id = $1 AND substr(started_at, 1, 7) = $2
               AND imported_entry_id IS NULL AND ignored = false
             ORDER BY started_at",
        )
        .bind(group_id)
        .bind(month)
        .fetch_all(pool)
        .await
        .map_err(|error| error.to_string())?;

    let mut created = 0u64;
    let total = open.len() as u64;
    let now = now_iso();

    for (session_id, started_at, ended_at, user_name, seconds, notes) in open {
        if seconds <= 0 {
            continue;
        }

        let entry_id = uuid::Uuid::new_v4().to_string();
        let note = format!(
            "TeamViewer{}",
            notes
                .as_deref()
                .map(|text| format!(": {}", single_line(text)))
                .unwrap_or_default()
        );

        sqlx::query(
            "INSERT INTO time_entries
                (id, customer_id, web_client_id, user_name, activity, note,
                 started_at, ended_at, duration_seconds, billable, source, created_at)
             VALUES ($1, $2, $3, $4, 'Fernwartung', $5, $6, $7, $8, $9, 'teamviewer', $10)",
        )
        .bind(&entry_id)
        .bind(&customer_id)
        .bind(&web_client_id)
        .bind(user_name.as_deref().unwrap_or("TeamViewer"))
        .bind(&note)
        .bind(&started_at)
        .bind(&ended_at)
        .bind(seconds)
        .bind(billable)
        .bind(&now)
        .execute(pool)
        .await
        .map_err(|error| error.to_string())?;

        sqlx::query("UPDATE teamviewer_sessions SET imported_entry_id = $1 WHERE id = $2")
            .bind(&entry_id)
            .bind(&session_id)
            .execute(pool)
            .await
            .map_err(|error| error.to_string())?;

        created += 1;
    }

    let skipped = total - created;

    Ok(ImportReport {
        created,
        skipped,
        message: if created == 0 {
            format!("Für {name} war in {month} nichts Offenes mehr übrig.")
        } else if skipped > 0 {
            format!(
                "{created} Verbindungen von {name} übernommen ({skipped} ohne Dauer übersprungen)."
            )
        } else {
            format!("{created} Verbindungen von {name} als Zeiteinträge übernommen.")
        },
    })
}

/// Mehrzeilige TeamViewer-Notizen auf eine Zeile bringen – der Kommentar im
/// Zeiteintrag ist einzeilig gedacht.
fn single_line(text: &str) -> String {
    let joined = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" · ");
    shorten(&joined, 400)
}

/// Nur Buchstaben und Ziffern, kleingeschrieben – „RA Reichelt" und
/// „RA-Reichelt" sollen gleich behandelt werden.
fn normalize(value: &str) -> String {
    value
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect()
}

/// Gleicher Name oder einer enthält den anderen (mindestens vier Zeichen).
fn matches(a: &str, b: &str) -> bool {
    if a.is_empty() || b.is_empty() {
        return false;
    }
    if a == b {
        return true;
    }
    let (short, long) = if a.len() <= b.len() { (a, b) } else { (b, a) };
    short.len() >= 4 && long.contains(short)
}

#[cfg(test)]
mod tests {
    use super::{
        edit_session, groups, import_sessions, matches, normalize, parse_time, sessions,
        single_line,
    };

    #[test]
    fn reads_both_timestamp_formats() {
        assert!(parse_time("2026-11-11T10:28:00Z").is_some());
        assert!(parse_time("20130116T194014Z").is_some());
        assert!(parse_time("11.11.2026 10:28").is_none());
    }

    #[test]
    fn names_match_across_spelling() {
        assert!(matches(
            &normalize("RA Reichelt"),
            &normalize("RA-Reichelt")
        ));
        assert!(matches(
            &normalize("Musterwerk"),
            &normalize("Musterwerk GmbH")
        ));
        // Zu kurze Übereinstimmungen sind kein Vorschlag.
        assert!(!matches(&normalize("RA"), &normalize("RA Reichelt")));
        assert!(!matches(&normalize("Leu"), &normalize("Musterwerk")));
    }


    // -----------------------------------------------------------------------
    // Bearbeiten einzelner Verbindungen
    // -----------------------------------------------------------------------

    async fn seed_session(pool: &sqlx::PgPool, id: &str, seconds: i64) {
        sqlx::query(
            "INSERT INTO teamviewer_groups (group_id, name, updated_at)
             VALUES ('g1', 'Musterwerk', '2026-11-01T00:00:00')
             ON CONFLICT (group_id) DO NOTHING",
        )
        .execute(pool)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO teamviewer_sessions
                (id, group_id, group_name, device_name, user_name, started_at, ended_at,
                 duration_seconds, fee, notes, synced_at)
             VALUES ($1, 'g1', 'Musterwerk', 'PC-1', 'Techniker', '2026-11-09T10:31:00',
                     '2026-11-09T10:41:00', $2, 0, 'Original-Notiz', '2026-11-09T12:00:00')",
        )
        .bind(id)
        .bind(seconds)
        .execute(pool)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn an_edited_note_replaces_the_original_but_keeps_it() {
        let (_guard, pool) = crate::testdb::fixture().await;
        seed_session(&pool, "s1", 600).await;

        edit_session(&pool, "s1", Some("Anruf dazwischen"), None, None)
            .await
            .unwrap();

        let rows = sessions(&pool, "2026-11", Some("g1")).await.unwrap();
        assert_eq!(rows[0].effective_note.as_deref(), Some("Anruf dazwischen"));
        // Das Original bleibt erhalten – sonst wäre die Herkunft verloren.
        assert_eq!(rows[0].notes.as_deref(), Some("Original-Notiz"));
    }

    #[tokio::test]
    async fn an_empty_note_restores_the_original() {
        let (_guard, pool) = crate::testdb::fixture().await;
        seed_session(&pool, "s1", 600).await;

        edit_session(&pool, "s1", Some("eigener Text"), None, None)
            .await
            .unwrap();
        edit_session(&pool, "s1", Some("   "), None, None).await.unwrap();

        let rows = sessions(&pool, "2026-11", Some("g1")).await.unwrap();
        assert_eq!(rows[0].effective_note.as_deref(), Some("Original-Notiz"));
        assert!(rows[0].note_edit.is_none());
    }

    #[tokio::test]
    async fn a_corrected_duration_keeps_the_original_visible() {
        let (_guard, pool) = crate::testdb::fixture().await;
        // 22 Stunden offen – vergessener Logout.
        seed_session(&pool, "s1", 22 * 3600).await;

        edit_session(&pool, "s1", None, Some(45 * 60), None)
            .await
            .unwrap();

        let rows = sessions(&pool, "2026-11", Some("g1")).await.unwrap();
        assert_eq!(rows[0].effective_seconds, 45 * 60);
        assert_eq!(rows[0].duration_seconds, 22 * 3600, "Original bleibt erhalten");
        assert!(!rows[0].unusual, "nach der Korrektur nicht mehr auffällig");
    }

    #[tokio::test]
    async fn zero_restores_the_connection_duration() {
        let (_guard, pool) = crate::testdb::fixture().await;
        seed_session(&pool, "s1", 600).await;

        edit_session(&pool, "s1", None, Some(60), None).await.unwrap();
        edit_session(&pool, "s1", None, Some(0), None).await.unwrap();

        let rows = sessions(&pool, "2026-11", Some("g1")).await.unwrap();
        assert_eq!(rows[0].effective_seconds, 600);
        assert!(rows[0].duration_override_seconds.is_none());
    }

    #[tokio::test]
    async fn an_absurd_duration_is_rejected() {
        let (_guard, pool) = crate::testdb::fixture().await;
        seed_session(&pool, "s1", 600).await;

        assert!(edit_session(&pool, "s1", None, Some(30 * 3600), None)
            .await
            .is_err());
    }


    #[tokio::test]
    async fn group_totals_use_the_corrected_duration() {
        let (_guard, pool) = crate::testdb::fixture().await;
        seed_session(&pool, "s1", 22 * 3600).await;

        edit_session(&pool, "s1", None, Some(45 * 60), None)
            .await
            .unwrap();

        let rows = groups(&pool, "2026-11").await.unwrap();
        assert_eq!(
            rows[0].open_seconds,
            45 * 60,
            "die Korrektur muss in die Gruppensumme eingehen, sonst stimmt              die Zahl im Reiter nicht mit den Sitzungen darunter"
        );
        assert_eq!(rows[0].seconds, 45 * 60);
    }

    #[tokio::test]
    async fn ignored_sessions_do_not_count_as_open() {
        let (_guard, pool) = crate::testdb::fixture().await;
        seed_session(&pool, "s1", 600).await;
        seed_session(&pool, "s2", 300).await;

        edit_session(&pool, "s2", None, None, Some(true)).await.unwrap();

        let groups = groups(&pool, "2026-11").await.unwrap();
        assert_eq!(groups[0].open_sessions, 1);
        assert_eq!(groups[0].open_seconds, 600);
    }

    #[tokio::test]
    async fn import_uses_the_corrections_and_skips_ignored() {
        let (_guard, pool) = crate::testdb::fixture().await;

        sqlx::query(
            "INSERT INTO customers (id, name, short_code, active, created_at)
             VALUES ('c1', 'Musterwerk GmbH', 'MWK', true, '2026-11-01T00:00:00')",
        )
        .execute(&pool)
        .await
        .unwrap();

        seed_session(&pool, "s1", 22 * 3600).await;
        seed_session(&pool, "s2", 300).await;
        sqlx::query(
            "UPDATE teamviewer_groups SET customer_id = 'c1' WHERE group_id = 'g1'",
        )
        .execute(&pool)
        .await
        .unwrap();

        edit_session(&pool, "s1", Some("Logout vergessen"), Some(45 * 60), None)
            .await
            .unwrap();
        edit_session(&pool, "s2", None, None, Some(true)).await.unwrap();

        let report = import_sessions(&pool, "g1", "2026-11", true).await.unwrap();
        assert_eq!(report.created, 1, "die ignorierte Sitzung bleibt draußen");

        let (seconds, note): (i64, String) = sqlx::query_as(
            "SELECT duration_seconds, COALESCE(note,'') FROM time_entries LIMIT 1",
        )
        .fetch_one(&pool)
        .await
        .unwrap();

        assert_eq!(seconds, 45 * 60, "die korrigierte Dauer zählt");
        assert!(note.contains("Logout vergessen"), "{note}");
    }

    #[tokio::test]
    async fn an_imported_session_cannot_be_edited_anymore() {
        let (_guard, pool) = crate::testdb::fixture().await;

        sqlx::query(
            "INSERT INTO customers (id, name, short_code, active, created_at)
             VALUES ('c1', 'Musterwerk GmbH', 'MWK', true, '2026-11-01T00:00:00')",
        )
        .execute(&pool)
        .await
        .unwrap();

        seed_session(&pool, "s1", 600).await;
        sqlx::query("UPDATE teamviewer_groups SET customer_id = 'c1' WHERE group_id = 'g1'")
            .execute(&pool)
            .await
            .unwrap();

        import_sessions(&pool, "g1", "2026-11", true).await.unwrap();

        let error = edit_session(&pool, "s1", Some("zu spät"), None, None)
            .await
            .unwrap_err();
        assert!(error.contains("Zeiten"), "{error}");
    }

    #[test]
    fn empty_or_multiline_tokens_are_rejected() {
        // Beides bricht vor dem Schreiben ab – es wird keine Datei angefasst.
        assert!(super::save_token("").is_err());
        assert!(super::save_token("   ").is_err());
        assert!(
            super::save_token("abc\ndef").is_err(),
            "Zeilenumbruch aus der Zwischenablage"
        );
        assert!(
            super::save_token("abc\rdef").is_err(),
            "Wagenrücklauf aus der Zwischenablage"
        );
    }

    #[test]
    fn notes_become_a_single_line() {
        assert_eq!(
            single_line("Backup\r\n> Kontrolle\r\n\r\n> Test"),
            "Backup · > Kontrolle · > Test"
        );
        assert_eq!(single_line(""), "");
    }
}
