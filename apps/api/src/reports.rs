//! Monatsbericht.
//!
//! Fasst für einen Monat zusammen, was je Kunde gearbeitet wurde: Wartungsläufe
//! mit Ergebnissen, Website-Aktionen, Zeiten mit Kommentaren und die daraus
//! folgende Abrechnung. Die Ausgabe ist bewusst als Dokument geschnitten –
//! im Frontend druckbar und als PDF speicherbar.

use serde::Serialize;
use sqlx::PgPool;

use crate::billing::with_vat;
use crate::settings::BillingSettings;
use crate::util::now_iso;

#[derive(Debug, Serialize)]
pub struct MonthlyReport {
    pub month: String,
    pub generated_at: String,
    pub generated_by: String,
    pub blocks: Vec<ReportBlock>,
    pub totals: ReportTotals,
}

#[derive(Debug, Serialize, Default)]
pub struct ReportTotals {
    pub customers: usize,
    pub billable_hours: f64,
    pub non_billable_hours: f64,
    pub time_amount: f64,
    pub flat_fees: f64,
    /// Nettosumme.
    pub total_amount: f64,
    pub vat_rate: f64,
    pub vat_amount: f64,
    pub gross_amount: f64,
    pub runs: usize,
    pub actions: usize,
}

#[derive(Debug, Serialize)]
pub struct ReportBlock {
    /// `ninjaone` oder `web`
    pub kind: String,
    pub customer_id: String,
    pub customer_name: String,
    pub short_code: String,
    pub contact_name: Option<String>,
    pub contact_email: Option<String>,
    pub contract_name: Option<String>,
    pub included_hours: f64,
    pub hourly_rate: f64,
    /// `contract` bei eigenem Vertragssatz, `standard` beim Standard-Satz.
    pub rate_source: String,
    pub flat_fee: f64,
    pub billable_hours: f64,
    pub non_billable_hours: f64,
    pub overage_hours: f64,
    pub time_amount: f64,
    /// Nettosumme des Kunden.
    pub total_amount: f64,
    pub vat_amount: f64,
    pub gross_amount: f64,
    pub sites: Vec<ReportSite>,
    pub runs: Vec<ReportRun>,
    pub entries: Vec<ReportEntry>,
}

#[derive(Debug, Serialize)]
pub struct ReportSite {
    pub label: String,
    pub url: String,
    pub wp_version: Option<String>,
    pub plugin_updates: i64,
    pub core_update: bool,
    pub actions: Vec<ReportAction>,
}

#[derive(Debug, Serialize)]
pub struct ReportAction {
    pub kind: String,
    pub label: String,
    pub taken_at: String,
    pub status: String,
    pub message: Option<String>,
    pub before_version: Option<String>,
    pub after_version: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ReportRun {
    pub title: String,
    pub taken_at: String,
    pub status: String,
    pub assignee: Option<String>,
    pub tasks: Vec<ReportTask>,
}

#[derive(Debug, Serialize)]
pub struct ReportTask {
    pub title: String,
    pub target: Option<String>,
    pub status: String,
    pub result: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ReportEntry {
    pub date: String,
    pub activity: String,
    pub comment: Option<String>,
    pub duration_seconds: i64,
    pub billable: bool,
    pub source: String,
    pub context: Option<String>,
    pub amount: f64,
}

#[derive(Debug, sqlx::FromRow)]
struct CustomerRow {
    id: String,
    name: String,
    short_code: String,
    contact_name: Option<String>,
    contact_email: Option<String>,
    contract_name: Option<String>,
    included_hours: f64,
    hourly_rate: f64,
    flat_fee: f64,
    billable_hours: f64,
    non_billable_hours: f64,
}

#[derive(Debug, sqlx::FromRow)]
struct WebRow {
    id: String,
    name: String,
    short_code: String,
    contact_name: Option<String>,
    contact_email: Option<String>,
    hourly_rate: f64,
    billable_hours: f64,
    non_billable_hours: f64,
    site_count: i64,
}

#[derive(Debug, sqlx::FromRow)]
struct SiteRow {
    id: String,
    label: Option<String>,
    url: String,
    wp_version: Option<String>,
    plugin_updates: i64,
    core_update: bool,
}

#[derive(Debug, sqlx::FromRow)]
struct ActionRow {
    site_id: String,
    kind: String,
    status: String,
    message: Option<String>,
    before_version: Option<String>,
    after_version: Option<String>,
    created_at: String,
}

#[derive(Debug, sqlx::FromRow)]
struct RunRow {
    id: String,
    title: String,
    scheduled_for: String,
    status: String,
    assignee: Option<String>,
}

#[derive(Debug, sqlx::FromRow)]
struct TaskRow {
    run_id: String,
    title: String,
    target_label: Option<String>,
    status: String,
    result_summary: Option<String>,
}

#[derive(Debug, sqlx::FromRow)]
struct EntryRow {
    started_at: String,
    activity: String,
    note: Option<String>,
    duration_seconds: i64,
    billable: bool,
    source: String,
    site_label: Option<String>,
    device_label: Option<String>,
}

/// Prüft `YYYY-MM` inklusive gültigem Monat (01–12).
pub fn valid_month(month: &str) -> bool {
    let bytes = month.as_bytes();
    if bytes.len() != 7 || bytes[4] != b'-' {
        return false;
    }

    let year = &month[..4];
    let number = &month[5..];

    year.bytes().all(|byte| byte.is_ascii_digit())
        && number.bytes().all(|byte| byte.is_ascii_digit())
        && matches!(number.parse::<u32>(), Ok(1..=12))
}

fn round2(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

fn action_label(kind: &str) -> String {
    match kind {
        "health" => "Health-Check".to_string(),
        "plugins" => "Plugin-Update".to_string(),
        "core" => "Core-Update".to_string(),
        other => other.to_string(),
    }
}

/// Baut den Bericht für einen Monat (`YYYY-MM`).
///
/// `include_idle = false` lässt Kunden weg, bei denen im Monat nichts passiert
/// ist – so enthält die Ablage nur, was auch abgerechnet wird.
pub async fn monthly(
    pool: &PgPool,
    month: &str,
    include_idle: bool,
    billing: &BillingSettings,
) -> Result<MonthlyReport, sqlx::Error> {
    let customers = sqlx::query_as::<_, CustomerRow>(
        r#"
        SELECT
            c.id, c.name, c.short_code, c.contact_name, c.contact_email,
            ct.name AS contract_name,
            CAST(COALESCE(ct.included_hours, 0) AS DOUBLE PRECISION) AS included_hours,
            CAST(COALESCE(ct.hourly_rate, 0) AS DOUBLE PRECISION) AS hourly_rate,
            CAST(COALESCE(ct.flat_fee, 0) AS DOUBLE PRECISION) AS flat_fee,
            COALESCE((
                SELECT SUM(te.duration_seconds) FROM time_entries te
                WHERE te.customer_id = c.id AND te.billable = true
                  AND substr(te.started_at, 1, 7) = $1
            ), 0)::double precision / 3600.0 AS billable_hours,
            COALESCE((
                SELECT SUM(te.duration_seconds) FROM time_entries te
                WHERE te.customer_id = c.id AND te.billable = false
                  AND substr(te.started_at, 1, 7) = $1
            ), 0)::double precision / 3600.0 AS non_billable_hours
        FROM customers c
        LEFT JOIN contracts ct ON ct.id = (
            SELECT id FROM contracts WHERE customer_id = c.id AND active = true LIMIT 1
        )
        WHERE c.active = true
        ORDER BY c.name
        "#,
    )
    .bind(month)
    .fetch_all(pool)
    .await?;

    let web_clients = sqlx::query_as::<_, WebRow>(
        r#"
        SELECT
            w.id, w.name, w.short_code, w.contact_name, w.contact_email,
            CAST(w.hourly_rate AS DOUBLE PRECISION) AS hourly_rate,
            COALESCE((
                SELECT SUM(te.duration_seconds) FROM time_entries te
                WHERE te.web_client_id = w.id AND te.billable = true
                  AND substr(te.started_at, 1, 7) = $1
            ), 0)::double precision / 3600.0 AS billable_hours,
            COALESCE((
                SELECT SUM(te.duration_seconds) FROM time_entries te
                WHERE te.web_client_id = w.id AND te.billable = false
                  AND substr(te.started_at, 1, 7) = $1
            ), 0)::double precision / 3600.0 AS non_billable_hours,
            (SELECT COUNT(*) FROM sites s WHERE s.web_client_id = w.id) AS site_count
        FROM web_clients w
        WHERE w.active = true
        ORDER BY w.name
        "#,
    )
    .bind(month)
    .fetch_all(pool)
    .await?;

    let mut blocks = Vec::new();

    for customer in customers {
        let runs = load_runs(pool, &customer.id, month).await?;
        let idle = customer.billable_hours <= 0.0
            && customer.non_billable_hours <= 0.0
            && runs.is_empty();

        if idle && !include_idle {
            continue;
        }

        let entries = load_entries(pool, Some(&customer.id), None, month, customer.hourly_rate).await?;

        let rate_source = if customer.hourly_rate > 0.0 { "contract" } else { "standard" };
        let hourly_rate = if customer.hourly_rate > 0.0 {
            customer.hourly_rate
        } else {
            billing.default_hourly_rate
        };

        let overage = (customer.billable_hours - customer.included_hours).max(0.0);
        let time_amount = overage * hourly_rate;
        // Auf Cent runden, bevor die Steuer darauf rechnet – sonst weicht die
        // Kundensumme um einen Cent von der Gesamtsumme ab.
        let net = round2(customer.flat_fee + time_amount);
        let (vat_amount, gross_amount) = with_vat(net, billing.vat_rate);

        blocks.push(ReportBlock {
            kind: "ninjaone".to_string(),
            customer_id: customer.id,
            customer_name: customer.name,
            short_code: customer.short_code,
            contact_name: customer.contact_name,
            contact_email: customer.contact_email,
            contract_name: customer.contract_name,
            included_hours: customer.included_hours,
            hourly_rate,
            rate_source: rate_source.to_string(),
            flat_fee: customer.flat_fee,
            billable_hours: round2(customer.billable_hours),
            non_billable_hours: round2(customer.non_billable_hours),
            overage_hours: round2(overage),
            time_amount: round2(time_amount),
            total_amount: net,
            vat_amount,
            gross_amount,
            sites: Vec::new(),
            runs,
            entries,
        });
    }

    for client in web_clients {
        let sites = load_sites(pool, &client.id, month).await?;
        let entries = load_entries(pool, None, Some(&client.id), month, client.hourly_rate).await?;

        let action_count: usize = sites.iter().map(|site| site.actions.len()).sum();
        let idle = client.billable_hours <= 0.0
            && client.non_billable_hours <= 0.0
            && action_count == 0;

        if idle && !include_idle {
            continue;
        }

        let rate_source = if client.hourly_rate > 0.0 { "contract" } else { "standard" };
        let hourly_rate = if client.hourly_rate > 0.0 {
            client.hourly_rate
        } else {
            billing.default_hourly_rate
        };

        let time_amount = client.billable_hours * hourly_rate;
        let (vat_amount, gross_amount) = with_vat(round2(time_amount), billing.vat_rate);

        blocks.push(ReportBlock {
            kind: "web".to_string(),
            customer_id: client.id,
            customer_name: client.name,
            short_code: client.short_code,
            contact_name: client.contact_name,
            contact_email: client.contact_email,
            contract_name: if client.site_count > 0 {
                Some(if client.site_count == 1 {
                    "Website-Betreuung".to_string()
                } else {
                    format!("Website-Betreuung · {} Seiten", client.site_count)
                })
            } else {
                None
            },
            included_hours: 0.0,
            hourly_rate,
            rate_source: rate_source.to_string(),
            flat_fee: 0.0,
            billable_hours: round2(client.billable_hours),
            non_billable_hours: round2(client.non_billable_hours),
            overage_hours: round2(client.billable_hours),
            time_amount: round2(time_amount),
            total_amount: round2(time_amount),
            vat_amount,
            gross_amount,
            sites,
            runs: Vec::new(),
            entries,
        });
    }

    let mut totals = ReportTotals {
        customers: blocks.len(),
        ..ReportTotals::default()
    };

    for block in &blocks {
        totals.billable_hours += block.billable_hours;
        totals.non_billable_hours += block.non_billable_hours;
        totals.time_amount += block.time_amount;
        totals.flat_fees += block.flat_fee;
        totals.total_amount += block.total_amount;
        totals.runs += block.runs.len();
        totals.actions += block.sites.iter().map(|site| site.actions.len()).sum::<usize>();
    }

    totals.billable_hours = round2(totals.billable_hours);
    totals.non_billable_hours = round2(totals.non_billable_hours);
    totals.time_amount = round2(totals.time_amount);
    totals.flat_fees = round2(totals.flat_fees);
    totals.total_amount = round2(totals.total_amount);

    let (vat_amount, gross_amount) = with_vat(totals.total_amount, billing.vat_rate);
    totals.vat_rate = billing.vat_rate;
    totals.vat_amount = vat_amount;
    totals.gross_amount = gross_amount;

    Ok(MonthlyReport {
        month: month.to_string(),
        generated_at: now_iso(),
        generated_by: String::new(),
        blocks,
        totals,
    })
}

async fn load_runs(
    pool: &PgPool,
    customer_id: &str,
    month: &str,
) -> Result<Vec<ReportRun>, sqlx::Error> {
    let runs = sqlx::query_as::<_, RunRow>(
        "SELECT id, title, scheduled_for, status, assignee
         FROM runs
         WHERE customer_id = $1 AND period = $2
         ORDER BY scheduled_for",
    )
    .bind(customer_id)
    .bind(month)
    .fetch_all(pool)
    .await?;

    if runs.is_empty() {
        return Ok(Vec::new());
    }

    let tasks = sqlx::query_as::<_, TaskRow>(
        "SELECT t.run_id, t.title, t.target_label, t.status, t.result_summary
         FROM tasks t
         JOIN runs r ON r.id = t.run_id
         WHERE r.customer_id = $1 AND r.period = $2
         ORDER BY t.position",
    )
    .bind(customer_id)
    .bind(month)
    .fetch_all(pool)
    .await?;

    Ok(runs
        .into_iter()
        .map(|run| ReportRun {
            title: run.title,
            taken_at: run.scheduled_for,
            status: run.status,
            assignee: run.assignee,
            tasks: tasks
                .iter()
                .filter(|task| task.run_id == run.id)
                .map(|task| ReportTask {
                    title: task.title.clone(),
                    target: task.target_label.clone(),
                    status: task.status.clone(),
                    result: task.result_summary.clone(),
                })
                .collect(),
        })
        .collect())
}

async fn load_sites(
    pool: &PgPool,
    web_client_id: &str,
    month: &str,
) -> Result<Vec<ReportSite>, sqlx::Error> {
    let sites = sqlx::query_as::<_, SiteRow>(
        "SELECT id, label, url, wp_version, plugin_updates, core_update
         FROM sites WHERE web_client_id = $1 ORDER BY label, url",
    )
    .bind(web_client_id)
    .fetch_all(pool)
    .await?;

    if sites.is_empty() {
        return Ok(Vec::new());
    }

    let actions = sqlx::query_as::<_, ActionRow>(
        "SELECT site_id, kind, status, message, before_version, after_version, created_at
         FROM web_actions
         WHERE web_client_id = $1 AND substr(created_at, 1, 7) = $2
         ORDER BY created_at",
    )
    .bind(web_client_id)
    .bind(month)
    .fetch_all(pool)
    .await?;

    Ok(sites
        .into_iter()
        .map(|site| ReportSite {
            label: site
                .label
                .clone()
                .unwrap_or_else(|| site.url.clone()),
            url: site.url,
            wp_version: site.wp_version,
            plugin_updates: site.plugin_updates,
            core_update: site.core_update,
            actions: actions
                .iter()
                .filter(|action| action.site_id == site.id)
                .map(|action| ReportAction {
                    kind: action.kind.clone(),
                    label: action_label(&action.kind),
                    taken_at: action.created_at.clone(),
                    status: action.status.clone(),
                    message: action.message.clone(),
                    before_version: action.before_version.clone(),
                    after_version: action.after_version.clone(),
                })
                .collect(),
        })
        .collect())
}

async fn load_entries(
    pool: &PgPool,
    customer_id: Option<&str>,
    web_client_id: Option<&str>,
    month: &str,
    hourly_rate: f64,
) -> Result<Vec<ReportEntry>, sqlx::Error> {
    let rows = sqlx::query_as::<_, EntryRow>(
        r#"
        SELECT
            te.started_at,
            te.activity,
            te.note,
            te.duration_seconds,
            te.billable,
            te.source,
            s.label AS site_label,
            d.hostname AS device_label
        FROM time_entries te
        LEFT JOIN sites s ON s.id = te.site_id
        LEFT JOIN devices d ON d.id = te.device_id
        WHERE substr(te.started_at, 1, 7) = $3
          AND (($1 IS NOT NULL AND te.customer_id = $1)
            OR ($2 IS NOT NULL AND te.web_client_id = $2))
        ORDER BY te.started_at
        "#,
    )
    .bind(customer_id)
    .bind(web_client_id)
    .bind(month)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| {
            let amount = if row.billable {
                (row.duration_seconds as f64 / 3600.0) * hourly_rate
            } else {
                0.0
            };

            ReportEntry {
                date: row.started_at,
                activity: row.activity,
                comment: row.note,
                duration_seconds: row.duration_seconds,
                billable: row.billable,
                source: row.source,
                context: row.device_label.or(row.site_label),
                amount: round2(amount),
            }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::valid_month;

    #[test]
    fn month_format_is_validated() {
        assert!(valid_month("2026-10"));
        assert!(valid_month("2026-01"));
        assert!(valid_month("2026-12"));

        assert!(!valid_month("2026-13"));
        assert!(!valid_month("2026-00"));
        assert!(!valid_month("2026-1"));
        assert!(!valid_month("20xx-10"));
        assert!(!valid_month("2026/10"));
        assert!(!valid_month(""));
        assert!(!valid_month("2026-1x"));
    }
}
