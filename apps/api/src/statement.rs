//! Aufstellung offener Leistungen eines Kunden.
//!
//! Anders als der Monatsbericht ist die Aufstellung nicht an einen Monat
//! gebunden: Sie sammelt alle Zeiten, die noch nicht als abgerechnet markiert
//! sind (`time_entries.billed_at IS NULL`). Damit lässt sich ein Kunde
//! abrechnen, auch wenn der Zeitraum über einen Monatswechsel läuft – und
//! nach dem Druck wird dieselbe Liste als abgerechnet markiert, sodass sie
//! beim nächsten Mal nicht wieder auftaucht.
//!
//! Der laufende Timer bleibt außen vor (`ended_at IS NULL`): Er hat noch
//! keine endgültige Dauer.

use serde::Serialize;
use sqlx::PgPool;

use crate::billing::with_vat;
use crate::settings::BillingSettings;
use crate::util::now_iso;

#[derive(Debug, Serialize)]
pub struct Statement {
    /// `customer` oder `web` – die Quelle der Zuordnung.
    pub kind: String,
    pub owner_id: String,
    pub name: String,
    pub short_code: String,
    pub contact_name: Option<String>,
    pub contact_email: Option<String>,
    pub phone: Option<String>,
    pub contract_name: Option<String>,
    /// Inklusivvolumen je Monat – die Aufstellung weist den vollen Aufwand aus.
    pub included_hours: f64,
    pub hourly_rate: f64,
    /// `contract` bei eigenem Vertragssatz, `standard` beim Standard-Satz.
    pub rate_source: String,
    pub generated_at: String,
    pub generated_by: String,
    /// Frühester und spätester Eintrag – der tatsächliche Zeitraum.
    pub first_date: Option<String>,
    pub last_date: Option<String>,
    pub entries: Vec<StatementEntry>,
    pub totals: StatementTotals,
}

#[derive(Debug, Serialize)]
pub struct StatementEntry {
    pub id: String,
    pub date: String,
    pub activity: String,
    pub comment: Option<String>,
    pub context: Option<String>,
    pub user_name: String,
    pub duration_seconds: i64,
    pub billable: bool,
    /// Nur bei `billable` gefüllt, sonst 0.
    pub amount: f64,
}

#[derive(Debug, Serialize, Default)]
pub struct StatementTotals {
    pub entries: usize,
    pub billable_seconds: i64,
    pub non_billable_seconds: i64,
    pub billable_hours: f64,
    pub non_billable_hours: f64,
    /// Nettosumme der abrechenbaren Zeiten.
    pub time_amount: f64,
    pub vat_rate: f64,
    pub vat_amount: f64,
    pub gross_amount: f64,
}

#[derive(Debug, sqlx::FromRow)]
struct OwnerRow {
    id: String,
    name: String,
    short_code: String,
    contact_name: Option<String>,
    contact_email: Option<String>,
    phone: Option<String>,
    contract_name: Option<String>,
    included_hours: f64,
    hourly_rate: f64,
}

#[derive(Debug, sqlx::FromRow)]
struct EntryRow {
    id: String,
    started_at: String,
    activity: String,
    note: Option<String>,
    user_name: String,
    duration_seconds: i64,
    billable: bool,
    site_label: Option<String>,
    device_label: Option<String>,
    task_title: Option<String>,
}

fn round2(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

/// Aufstellung für einen NinjaOne-Kunden. `None`, wenn es ihn nicht gibt.
pub async fn for_customer(
    pool: &PgPool,
    customer_id: &str,
    billing: &BillingSettings,
) -> Result<Option<Statement>, sqlx::Error> {
    let owner = sqlx::query_as::<_, OwnerRow>(
        r#"
        SELECT
            c.id, c.name, c.short_code, c.contact_name, c.contact_email, c.phone,
            ct.name AS contract_name,
            CAST(COALESCE(ct.included_hours, 0) AS DOUBLE PRECISION) AS included_hours,
            CAST(COALESCE(ct.hourly_rate, 0) AS DOUBLE PRECISION) AS hourly_rate
        FROM customers c
        LEFT JOIN contracts ct ON ct.id = (
            SELECT id FROM contracts WHERE customer_id = c.id AND active = true LIMIT 1
        )
        WHERE c.id = $1
        "#,
    )
    .bind(customer_id)
    .fetch_optional(pool)
    .await?;

    let Some(owner) = owner else {
        return Ok(None);
    };

    let rows = sqlx::query_as::<_, EntryRow>(&format!(
        "{OPEN_ENTRIES} AND te.customer_id = $1 ORDER BY te.started_at"
    ))
    .bind(customer_id)
    .fetch_all(pool)
    .await?;

    Ok(Some(build("customer", owner, rows, billing)))
}

/// Aufstellung für einen Web-Kunden. `None`, wenn es ihn nicht gibt.
pub async fn for_web_client(
    pool: &PgPool,
    web_client_id: &str,
    billing: &BillingSettings,
) -> Result<Option<Statement>, sqlx::Error> {
    let owner = sqlx::query_as::<_, OwnerRow>(
        r#"
        SELECT
            w.id, w.name, w.short_code, w.contact_name, w.contact_email, w.phone,
            NULL::text AS contract_name,
            0.0::double precision AS included_hours,
            CAST(w.hourly_rate AS DOUBLE PRECISION) AS hourly_rate
        FROM web_clients w
        WHERE w.id = $1
        "#,
    )
    .bind(web_client_id)
    .fetch_optional(pool)
    .await?;

    let Some(owner) = owner else {
        return Ok(None);
    };

    let rows = sqlx::query_as::<_, EntryRow>(&format!(
        "{OPEN_ENTRIES} AND te.web_client_id = $1 ORDER BY te.started_at"
    ))
    .bind(web_client_id)
    .fetch_all(pool)
    .await?;

    Ok(Some(build("web", owner, rows, billing)))
}

const OPEN_ENTRIES: &str = r#"
    SELECT
        te.id,
        te.started_at,
        te.activity,
        te.note,
        te.user_name,
        te.duration_seconds,
        te.billable,
        s.label AS site_label,
        d.hostname AS device_label,
        t.title AS task_title
    FROM time_entries te
    LEFT JOIN sites s ON s.id = te.site_id
    LEFT JOIN devices d ON d.id = te.device_id
    LEFT JOIN tasks t ON t.id = te.task_id
    WHERE te.billed_at IS NULL AND te.ended_at IS NOT NULL
"#;

fn build(kind: &str, owner: OwnerRow, rows: Vec<EntryRow>, billing: &BillingSettings) -> Statement {
    let rate_source = if owner.hourly_rate > 0.0 {
        "contract"
    } else {
        "standard"
    };
    let hourly_rate = if owner.hourly_rate > 0.0 {
        owner.hourly_rate
    } else {
        billing.default_hourly_rate
    };

    let mut totals = StatementTotals {
        vat_rate: billing.vat_rate,
        ..StatementTotals::default()
    };
    let mut entries = Vec::with_capacity(rows.len());

    for row in rows {
        let amount = if row.billable {
            round2(row.duration_seconds as f64 / 3600.0 * hourly_rate)
        } else {
            0.0
        };

        if row.billable {
            totals.billable_seconds += row.duration_seconds;
            totals.time_amount += amount;
        } else {
            totals.non_billable_seconds += row.duration_seconds;
        }

        // Ein Eintrag kann gleichzeitig am Gerät und an einer Site hängen –
        // die Gerätebezeichnung ist für die Rechnung die aussagekräftigere.
        let context = row
            .device_label
            .or(row.site_label)
            .or(row.task_title);

        entries.push(StatementEntry {
            id: row.id,
            date: row.started_at,
            activity: row.activity,
            comment: row.note,
            context,
            user_name: row.user_name,
            duration_seconds: row.duration_seconds,
            billable: row.billable,
            amount,
        });
    }

    let first_date = entries.first().map(|entry| entry.date.clone());
    let last_date = entries.last().map(|entry| entry.date.clone());

    totals.entries = entries.len();
    totals.billable_hours = round2(totals.billable_seconds as f64 / 3600.0);
    totals.non_billable_hours = round2(totals.non_billable_seconds as f64 / 3600.0);
    totals.time_amount = round2(totals.time_amount);

    let (vat_amount, gross_amount) = with_vat(totals.time_amount, billing.vat_rate);
    totals.vat_amount = vat_amount;
    totals.gross_amount = gross_amount;

    Statement {
        kind: kind.to_string(),
        owner_id: owner.id,
        name: owner.name,
        short_code: owner.short_code,
        contact_name: owner.contact_name,
        contact_email: owner.contact_email,
        phone: owner.phone,
        contract_name: owner.contract_name,
        included_hours: owner.included_hours,
        hourly_rate,
        rate_source: rate_source.to_string(),
        generated_at: now_iso(),
        generated_by: String::new(),
        first_date,
        last_date,
        entries,
        totals,
    }
}

/// Markiert die übergebenen Einträge als abgerechnet.
///
/// Die Bedingung `billed_at IS NULL` macht den Aufruf wiederholbar: Ein
/// doppelter Klick oder ein zweites Fenster überschreibt den ursprünglichen
/// Zeitpunkt nicht. Die Zuordnung zum Kunden verhindert, dass fremde
/// Einträge über eine geratene ID abgerechnet werden.
/// Rückgabe: wie viele Einträge tatsächlich neu markiert wurden.
pub async fn mark_billed(
    pool: &PgPool,
    kind: &str,
    owner_id: &str,
    entry_ids: &[String],
) -> Result<u64, sqlx::Error> {
    if entry_ids.is_empty() {
        return Ok(0);
    }

    let column = if kind == "web" {
        "web_client_id"
    } else {
        "customer_id"
    };
    let sql = format!(
        "UPDATE time_entries SET billed_at = $1
         WHERE billed_at IS NULL AND {column} = $2 AND id = ANY($3)"
    );

    let result = sqlx::query(&sql)
        .bind(now_iso())
        .bind(owner_id)
        .bind(entry_ids)
        .execute(pool)
        .await?;

    Ok(result.rows_affected())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testdb;

    async fn customer(pool: &PgPool, id: &str, name: &str, rate: f64) {
        let now = now_iso();
        sqlx::query(
            "INSERT INTO customers (id, name, short_code, active, created_at)
             VALUES ($1, $2, 'TST', true, $3)",
        )
        .bind(id)
        .bind(name)
        .bind(&now)
        .execute(pool)
        .await
        .unwrap();

        if rate > 0.0 {
            sqlx::query(
                "INSERT INTO contracts (id, customer_id, name, included_hours, hourly_rate, flat_fee, active)
                 VALUES ($1, $2, 'Wartung', 0, $3, 0, true)",
            )
            .bind(format!("ct-{id}"))
            .bind(id)
            .bind(rate)
            .execute(pool)
            .await
            .unwrap();
        }
    }

    async fn entry(
        pool: &PgPool,
        id: &str,
        customer_id: &str,
        seconds: i64,
        billable: bool,
        billed_at: Option<&str>,
    ) {
        sqlx::query(
            "INSERT INTO time_entries
                (id, customer_id, user_name, activity, started_at, ended_at,
                 duration_seconds, billable, source, created_at, billed_at)
             VALUES ($1, $2, 'S. Wolf', 'Fernwartung', '2026-10-05T09:00:00',
                     '2026-10-05T10:00:00', $3, $4, 'timer', '2026-10-05T10:00:00', $5)",
        )
        .bind(id)
        .bind(customer_id)
        .bind(seconds)
        .bind(billable)
        .bind(billed_at)
        .execute(pool)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn statement_lists_only_open_entries() {
        let (_guard, pool) = testdb::fixture().await;
        customer(&pool, "k1", "Muster GmbH", 100.0).await;
        entry(&pool, "e1", "k1", 3600, true, None).await;
        entry(&pool, "e2", "k1", 1800, true, Some("2026-10-01T00:00:00")).await;
        entry(&pool, "e3", "k1", 900, false, None).await;

        let statement = for_customer(&pool, "k1", &BillingSettings::default())
            .await
            .unwrap()
            .unwrap();

        assert_eq!(statement.entries.len(), 2, "abgerechnete Zeit bleibt weg");
        assert_eq!(statement.totals.billable_seconds, 3600);
        assert_eq!(statement.totals.non_billable_seconds, 900);
        assert_eq!(statement.totals.time_amount, 100.0);
        assert_eq!(statement.rate_source, "contract");
    }

    #[tokio::test]
    async fn statement_uses_standard_rate_without_contract() {
        let (_guard, pool) = testdb::fixture().await;
        customer(&pool, "k2", "Ohne Vertrag", 0.0).await;
        entry(&pool, "e1", "k2", 7200, true, None).await;

        let billing = BillingSettings {
            default_hourly_rate: 90.0,
            vat_rate: 19.0,
        };
        let statement = for_customer(&pool, "k2", &billing).await.unwrap().unwrap();

        assert_eq!(statement.rate_source, "standard");
        assert_eq!(statement.hourly_rate, 90.0);
        assert_eq!(statement.totals.time_amount, 180.0);
        assert_eq!(statement.totals.gross_amount, 214.2);
    }

    #[tokio::test]
    async fn entries_are_listed_chronologically() {
        let (_guard, pool) = testdb::fixture().await;
        customer(&pool, "k7", "Zeiträume", 100.0).await;

        // Absichtlich in falscher Reihenfolge angelegt: Die Aufstellung muss
        // nach Datum sortieren, damit der Zeitraum im Kopf stimmt.
        for (id, day) in [("e1", "2026-11-03"), ("e2", "2026-09-18"), ("e3", "2026-10-21")] {
            sqlx::query(
                "INSERT INTO time_entries
                    (id, customer_id, user_name, activity, started_at, ended_at,
                     duration_seconds, billable, source, created_at)
                 VALUES ($1, 'k7', 'S. Wolf', 'Fernwartung', $2, $2, 3600, true, 'timer', $2)",
            )
            .bind(id)
            .bind(format!("{day}T09:00:00"))
            .execute(&pool)
            .await
            .unwrap();
        }

        let statement = for_customer(&pool, "k7", &BillingSettings::default())
            .await
            .unwrap()
            .unwrap();

        let dates: Vec<&str> = statement.entries.iter().map(|e| e.date.as_str()).collect();
        assert_eq!(
            dates,
            vec![
                "2026-09-18T09:00:00",
                "2026-10-21T09:00:00",
                "2026-11-03T09:00:00"
            ]
        );
        assert_eq!(statement.first_date.as_deref(), Some("2026-09-18T09:00:00"));
        assert_eq!(statement.last_date.as_deref(), Some("2026-11-03T09:00:00"));
    }

    #[tokio::test]
    async fn marking_billed_clears_the_statement_and_is_repeatable() {
        let (_guard, pool) = testdb::fixture().await;
        customer(&pool, "k3", "Kunde Drei", 100.0).await;
        entry(&pool, "e1", "k3", 3600, true, None).await;
        entry(&pool, "e2", "k3", 600, true, None).await;

        let statement = for_customer(&pool, "k3", &BillingSettings::default())
            .await
            .unwrap()
            .unwrap();
        let ids: Vec<String> = statement.entries.iter().map(|e| e.id.clone()).collect();

        assert_eq!(mark_billed(&pool, "customer", "k3", &ids).await.unwrap(), 2);
        assert_eq!(mark_billed(&pool, "customer", "k3", &ids).await.unwrap(), 0);

        let after = for_customer(&pool, "k3", &BillingSettings::default())
            .await
            .unwrap()
            .unwrap();
        assert!(after.entries.is_empty());
        assert_eq!(after.totals.time_amount, 0.0);
    }

    #[tokio::test]
    async fn marking_billed_does_not_touch_other_customers() {
        let (_guard, pool) = testdb::fixture().await;
        customer(&pool, "k4", "Kunde Vier", 100.0).await;
        customer(&pool, "k5", "Kunde Fünf", 100.0).await;
        entry(&pool, "e1", "k4", 3600, true, None).await;
        entry(&pool, "e2", "k5", 3600, true, None).await;

        let marked = mark_billed(&pool, "customer", "k4", &["e1".to_string(), "e2".to_string()])
            .await
            .unwrap();

        assert_eq!(marked, 1);
        let other = for_customer(&pool, "k5", &BillingSettings::default())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(other.entries.len(), 1);
    }

    #[tokio::test]
    async fn running_timer_is_not_part_of_the_statement() {
        let (_guard, pool) = testdb::fixture().await;
        customer(&pool, "k6", "Laufender Timer", 100.0).await;
        sqlx::query(
            "INSERT INTO time_entries
                (id, customer_id, user_name, activity, started_at, duration_seconds,
                 billable, source, created_at)
             VALUES ('e1', 'k6', 'S. Wolf', 'Fernwartung', '2026-10-05T09:00:00', 0,
                     true, 'timer', '2026-10-05T09:00:00')",
        )
        .execute(&pool)
        .await
        .unwrap();

        let statement = for_customer(&pool, "k6", &BillingSettings::default())
            .await
            .unwrap()
            .unwrap();

        assert!(statement.entries.is_empty());
    }
}
