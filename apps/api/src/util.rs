use chrono::{DateTime, Local, SecondsFormat, TimeZone, Utc};
use sqlx::PgPool;
use uuid::Uuid;

pub fn now_iso() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

pub fn current_month() -> String {
    Local::now().format("%Y-%m").to_string()
}

/// „2026-10" → „Oktober 2026". Unbekannte Formate kommen unverändert zurück.
pub fn month_label(month: &str) -> String {
    const NAMES: [&str; 12] = [
        "Januar",
        "Februar",
        "März",
        "April",
        "Mai",
        "Juni",
        "Juli",
        "August",
        "September",
        "Oktober",
        "November",
        "Dezember",
    ];

    let parts: Vec<&str> = month.split('-').collect();
    if parts.len() != 2 {
        return month.to_string();
    }

    match parts[1].parse::<usize>() {
        Ok(number) if (1..=12).contains(&number) => format!("{} {}", NAMES[number - 1], parts[0]),
        _ => month.to_string(),
    }
}

pub fn parse_timestamp(value: &str) -> Option<DateTime<Utc>> {
    if let Ok(parsed) = DateTime::parse_from_rfc3339(value) {
        return Some(parsed.with_timezone(&Utc));
    }

    if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S") {
        return Some(Utc.from_utc_datetime(&naive));
    }

    if let Ok(date) = chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d") {
        return date
            .and_hms_opt(0, 0, 0)
            .map(|naive| Utc.from_utc_datetime(&naive));
    }

    None
}

pub async fn audit(
    pool: &PgPool,
    actor: &str,
    action: &str,
    entity_type: Option<&str>,
    entity_id: Option<&str>,
    detail: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO audit_events (id, actor, action, entity_type, entity_id, detail, created_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(actor)
    .bind(action)
    .bind(entity_type)
    .bind(entity_id)
    .bind(detail)
    .bind(now_iso())
    .execute(pool)
    .await?;

    Ok(())
}

pub fn german_date(value: &str) -> String {
    if let Some(parsed) = parse_timestamp(value) {
        return parsed.with_timezone(&Local).format("%d.%m.%Y").to_string();
    }
    value.to_string()
}

pub fn german_decimal(value: f64, decimals: usize) -> String {
    format!("{value:.decimals$}").replace('.', ",")
}
