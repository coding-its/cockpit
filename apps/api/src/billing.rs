use sqlx::PgPool;

use crate::models::{BillingCustomer, BillingSummary};

pub use crate::settings::FALLBACK_HOURLY_RATE as DEFAULT_HOURLY_RATE;

#[derive(Debug, sqlx::FromRow)]
struct BillingRow {
    customer_id: String,
    customer_name: String,
    short_code: String,
    included_hours: f64,
    hourly_rate: f64,
    flat_fee: f64,
    billable_hours: f64,
    non_billable_hours: f64,
}

/// `default_rate` greift, wo im Vertrag kein Stundensatz gepflegt ist.
pub async fn summary(
    pool: &PgPool,
    month: &str,
    focus_only: bool,
    default_rate: f64,
) -> Result<BillingSummary, sqlx::Error> {
    let rows = sqlx::query_as::<_, BillingRow>(
        r#"
        SELECT * FROM (
        SELECT
            c.id AS customer_id,
            c.name AS customer_name,
            c.short_code,
            CAST(COALESCE(ct.included_hours, 0) AS DOUBLE PRECISION) AS included_hours,
            CAST(COALESCE(ct.hourly_rate, 0) AS DOUBLE PRECISION) AS hourly_rate,
            CAST(COALESCE(ct.flat_fee, 0) AS DOUBLE PRECISION) AS flat_fee,
            COALESCE(SUM(CASE WHEN te.billable = true THEN te.duration_seconds ELSE 0 END), 0)::double precision / 3600.0 AS billable_hours,
            COALESCE(SUM(CASE WHEN te.billable = false THEN te.duration_seconds ELSE 0 END), 0)::double precision / 3600.0 AS non_billable_hours
        FROM customers c
        LEFT JOIN contracts ct ON ct.id = (
            SELECT id FROM contracts WHERE customer_id = c.id AND active = true LIMIT 1
        )
        LEFT JOIN time_entries te
            ON te.customer_id = c.id
            AND substr(te.started_at, 1, 7) = $1
        WHERE c.active = true
          AND (NOT $2 OR c.focus = true)
        GROUP BY c.id, c.name, c.short_code, ct.included_hours, ct.hourly_rate, ct.flat_fee

        UNION ALL

        SELECT
            w.id AS customer_id,
            w.name AS customer_name,
            w.short_code,
            0.0 AS included_hours,
            CAST(w.hourly_rate AS DOUBLE PRECISION) AS hourly_rate,
            0.0 AS flat_fee,
            COALESCE(SUM(CASE WHEN te.billable = true THEN te.duration_seconds ELSE 0 END), 0)::double precision / 3600.0 AS billable_hours,
            COALESCE(SUM(CASE WHEN te.billable = false THEN te.duration_seconds ELSE 0 END), 0)::double precision / 3600.0 AS non_billable_hours
        FROM web_clients w
        LEFT JOIN time_entries te
            ON te.web_client_id = w.id
            AND substr(te.started_at, 1, 7) = $1
        WHERE w.active = true
        GROUP BY w.id, w.name, w.short_code, w.hourly_rate
        )
        ORDER BY customer_name
        "#,
    )
    .bind(month)
    .bind(focus_only)
    .fetch_all(pool)
    .await?;

    let mut customers = Vec::with_capacity(rows.len());
    let mut total_billable_hours = 0.0;
    let mut total_amount = 0.0;

    for row in rows {
        let uses_default = row.hourly_rate <= 0.0;
        let hourly_rate = if uses_default {
            default_rate
        } else {
            row.hourly_rate
        };

        let overage_hours = (row.billable_hours - row.included_hours).max(0.0);
        let time_amount = overage_hours * hourly_rate;
        let total = row.flat_fee + time_amount;

        total_billable_hours += row.billable_hours;
        total_amount += total;

        customers.push(BillingCustomer {
            customer_id: row.customer_id,
            customer_name: row.customer_name,
            short_code: row.short_code,
            billable_hours: round2(row.billable_hours),
            non_billable_hours: round2(row.non_billable_hours),
            included_hours: row.included_hours,
            hourly_rate,
            default_rate: uses_default,
            flat_fee: row.flat_fee,
            overage_hours: round2(overage_hours),
            time_amount: round2(time_amount),
            total_amount: round2(total),
        });
    }

    Ok(BillingSummary {
        month: month.to_string(),
        customers,
        total_billable_hours: round2(total_billable_hours),
        total_amount: round2(total_amount),
    })
}

/// Netto- und Bruttobetrag zu einem Nettobetrag, mit übergebenem Steuersatz.
pub fn with_vat(net: f64, vat_rate: f64) -> (f64, f64) {
    let vat = net * vat_rate / 100.0;
    (round2(vat), round2(net + vat))
}

fn round2(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}
