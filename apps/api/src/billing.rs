use sqlx::PgPool;

use crate::models::{BillingCustomer, BillingSummary};
use crate::reports::{overage_seconds, seconds_to_hours};

pub use crate::settings::FALLBACK_HOURLY_RATE as DEFAULT_HOURLY_RATE;

#[derive(Debug, sqlx::FromRow)]
struct BillingRow {
    customer_id: String,
    customer_name: String,
    short_code: String,
    included_hours: f64,
    hourly_rate: f64,
    flat_fee: f64,
    billable_seconds: i64,
    non_billable_seconds: i64,
    /// Summe der auf Cent gerundeten Zeilenbeträge mit dem eigenen Satz.
    amount_own_rate: f64,
    /// Dieselbe Summe, gerechnet mit dem Standard-Satz.
    amount_default_rate: f64,
}

/// `default_rate` greift, wo im Vertrag kein Stundensatz gepflegt ist.
///
/// Die Beträge entstehen je Zeiteintrag und werden erst danach addiert – so
/// ergibt die Summe der Positionen genau den ausgewiesenen Betrag, wie im
/// Bericht und in der Aufstellung.
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
            COALESCE(SUM(CASE WHEN te.billable = true THEN te.duration_seconds ELSE 0 END), 0)::bigint AS billable_seconds,
            COALESCE(SUM(CASE WHEN te.billable = false THEN te.duration_seconds ELSE 0 END), 0)::bigint AS non_billable_seconds,
            COALESCE(SUM(CASE WHEN te.billable = true THEN
                round(te.duration_seconds * COALESCE(ct.hourly_rate, 0) / 3600.0 * 100) / 100
            ELSE 0 END), 0)::double precision AS amount_own_rate,
            COALESCE(SUM(CASE WHEN te.billable = true THEN
                round(te.duration_seconds * $3 / 3600.0 * 100) / 100
            ELSE 0 END), 0)::double precision AS amount_default_rate
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
            COALESCE(SUM(CASE WHEN te.billable = true THEN te.duration_seconds ELSE 0 END), 0)::bigint AS billable_seconds,
            COALESCE(SUM(CASE WHEN te.billable = false THEN te.duration_seconds ELSE 0 END), 0)::bigint AS non_billable_seconds,
            COALESCE(SUM(CASE WHEN te.billable = true THEN
                round(te.duration_seconds * COALESCE(w.hourly_rate, 0) / 3600.0 * 100) / 100
            ELSE 0 END), 0)::double precision AS amount_own_rate,
            COALESCE(SUM(CASE WHEN te.billable = true THEN
                round(te.duration_seconds * $3 / 3600.0 * 100) / 100
            ELSE 0 END), 0)::double precision AS amount_default_rate
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
    .bind(default_rate)
    .fetch_all(pool)
    .await?;

    let mut customers = Vec::with_capacity(rows.len());
    let mut total_billable_seconds = 0i64;
    let mut total_amount = 0.0;

    for row in rows {
        let uses_default = row.hourly_rate <= 0.0;
        let hourly_rate = if uses_default {
            default_rate
        } else {
            row.hourly_rate
        };

        // Rechtsgrundlage der Beträge: der eigene Satz, sonst der Standard-Satz.
        let lines_total = if uses_default {
            row.amount_default_rate
        } else {
            row.amount_own_rate
        };
        let included_value = round2(row.included_hours * hourly_rate);
        let time_amount = round2((lines_total - included_value).max(0.0));
        let total = round2(row.flat_fee + time_amount);
        let overage = overage_seconds(row.billable_seconds, row.included_hours);

        total_billable_seconds += row.billable_seconds;
        total_amount += total;

        customers.push(BillingCustomer {
            customer_id: row.customer_id,
            customer_name: row.customer_name,
            short_code: row.short_code,
            billable_seconds: row.billable_seconds,
            non_billable_seconds: row.non_billable_seconds,
            billable_hours: seconds_to_hours(row.billable_seconds),
            non_billable_hours: seconds_to_hours(row.non_billable_seconds),
            included_hours: row.included_hours,
            hourly_rate,
            default_rate: uses_default,
            flat_fee: row.flat_fee,
            overage_seconds: overage,
            overage_hours: seconds_to_hours(overage),
            time_amount,
            total_amount: total,
        });
    }

    Ok(BillingSummary {
        month: month.to_string(),
        customers,
        total_billable_seconds,
        total_billable_hours: seconds_to_hours(total_billable_seconds),
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

#[cfg(test)]
mod tests {
    use super::{summary, with_vat};
    use crate::settings::BillingSettings;
    use crate::testdb;
    use crate::util::now_iso;
    use sqlx::PgPool;

    async fn customer(pool: &PgPool, id: &str, rate: f64, included: f64) {
        sqlx::query(
            "INSERT INTO customers (id, name, short_code, active, focus, created_at)
             VALUES ($1, $1, 'TST', true, true, $2)",
        )
        .bind(id)
        .bind(now_iso())
        .execute(pool)
        .await
        .unwrap();

        if rate > 0.0 || included > 0.0 {
            sqlx::query(
                "INSERT INTO contracts (id, customer_id, name, included_hours, hourly_rate, flat_fee, active)
                 VALUES ($1, $2, 'Wartung', $3, $4, 0, true)",
            )
            .bind(format!("ct-{id}"))
            .bind(id)
            .bind(included)
            .bind(rate)
            .execute(pool)
            .await
            .unwrap();
        }
    }

    async fn entry(pool: &PgPool, id: &str, customer_id: &str, seconds: i64) {
        sqlx::query(
            "INSERT INTO time_entries
                (id, customer_id, user_name, activity, started_at, ended_at,
                 duration_seconds, billable, source, created_at)
             VALUES ($1, $2, 'S. Wolf', 'Gerätewartung', '2026-11-03T06:54:00',
                     '2026-11-03T06:55:00', $3, true, 'timer', $4)",
        )
        .bind(id)
        .bind(customer_id)
        .bind(seconds)
        .bind(now_iso())
        .execute(pool)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn amount_is_the_sum_of_the_rounded_entries() {
        let (_guard, pool) = testdb::fixture().await;
        customer(&pool, "k1", 126.05, 0.0).await;
        entry(&pool, "e1", "k1", 3).await;
        entry(&pool, "e2", "k1", 24).await;
        entry(&pool, "e3", "k1", 16).await;
        entry(&pool, "e4", "k1", 5400).await;

        let result = summary(&pool, "2026-11", false, 126.05).await.unwrap();
        let row = &result.customers[0];

        // 0,11 + 0,84 + 0,56 + 189,08 – jede Position auf Cent gerundet.
        assert_eq!(row.time_amount, 190.59);
        assert_eq!(row.billable_seconds, 5443);
        assert_eq!(result.total_billable_seconds, 5443);
    }

    #[tokio::test]
    async fn standard_rate_applies_without_a_contract() {
        let (_guard, pool) = testdb::fixture().await;
        customer(&pool, "k2", 0.0, 0.0).await;
        entry(&pool, "e1", "k2", 3600).await;

        let result = summary(&pool, "2026-11", false, 100.0).await.unwrap();
        let row = &result.customers[0];

        assert!(row.default_rate);
        assert_eq!(row.hourly_rate, 100.0);
        assert_eq!(row.time_amount, 100.0);
    }

    #[tokio::test]
    async fn included_hours_reduce_the_amount_but_not_the_duration() {
        let (_guard, pool) = testdb::fixture().await;
        customer(&pool, "k3", 100.0, 2.0).await;
        entry(&pool, "e1", "k3", 3 * 3600).await;

        let result = summary(&pool, "2026-11", false, 100.0).await.unwrap();
        let row = &result.customers[0];

        assert_eq!(row.billable_seconds, 3 * 3600);
        assert_eq!(row.overage_seconds, 3600);
        assert_eq!(row.time_amount, 100.0);
    }

    #[tokio::test]
    async fn nothing_is_billed_when_the_included_volume_is_not_reached() {
        let (_guard, pool) = testdb::fixture().await;
        customer(&pool, "k4", 100.0, 2.0).await;
        entry(&pool, "e1", "k4", 7170).await;

        let result = summary(&pool, "2026-11", false, 100.0).await.unwrap();
        let row = &result.customers[0];

        assert_eq!(row.overage_seconds, 0);
        assert_eq!(row.time_amount, 0.0);
    }

    #[test]
    fn vat_is_computed_on_the_net_amount() {
        assert_eq!(with_vat(190.59, 19.0), (36.21, 226.8));
        assert_eq!(with_vat(0.0, 19.0), (0.0, 0.0));
    }

    #[test]
    fn defaults_are_sane() {
        let settings = BillingSettings::default();
        assert!(settings.default_hourly_rate > 0.0);
        assert_eq!(settings.vat_rate, 19.0);
    }
}
