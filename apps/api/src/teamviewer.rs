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
    pub duration_seconds: i64,
    pub fee: f64,
    pub notes: Option<String>,
    pub imported_entry_id: Option<String>,
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
                AND s.imported_entry_id IS NULL)::bigint AS open_sessions,
            COALESCE((SELECT SUM(s.duration_seconds) FROM teamviewer_sessions s
              WHERE s.group_id = g.group_id AND substr(s.started_at, 1, 7) = $1), 0)::bigint AS seconds,
            COALESCE((SELECT SUM(s.duration_seconds) FROM teamviewer_sessions s
              WHERE s.group_id = g.group_id AND substr(s.started_at, 1, 7) = $1
                AND s.imported_entry_id IS NULL), 0)::bigint AS open_seconds,
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
            started_at, ended_at, duration_seconds, fee, notes, imported_entry_id,
            (duration_seconds >= $3) AS unusual
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

    let open: Vec<(String, String, Option<String>, Option<String>, i64, Option<String>)> =
        sqlx::query_as(
            "SELECT id, started_at, ended_at, user_name, duration_seconds, notes
             FROM teamviewer_sessions
             WHERE group_id = $1 AND substr(started_at, 1, 7) = $2
               AND imported_entry_id IS NULL
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
    use super::{matches, normalize, parse_time, single_line};

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

    #[test]
    fn notes_become_a_single_line() {
        assert_eq!(
            single_line("Backup\r\n> Kontrolle\r\n\r\n> Test"),
            "Backup · > Kontrolle · > Test"
        );
        assert_eq!(single_line(""), "");
    }
}
