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
    /// Sekundengenau, damit sich jeder Betrag nachrechnen lässt. Die Stunden
    /// daneben sind nur die gerundete Anzeige derselben Zahl.
    pub billable_seconds: i64,
    pub non_billable_seconds: i64,
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
    /// Grundlage aller Beträge: die tatsächlich erfassten Sekunden.
    pub billable_seconds: i64,
    pub non_billable_seconds: i64,
    /// Überhang über die Inklusivstunden, ebenfalls in Sekunden.
    pub overage_seconds: i64,
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
    /// Abrechnungskonto, über das dieser Block abgerechnet wird.
    pub account_id: Option<String>,
    pub account_code: Option<String>,
    /// Namen der Datensätze, aus denen der Block besteht (bei Konten > 1).
    pub members: Vec<String>,
    /// Mehr als ein Vertrag im Konto – sollte nicht vorkommen.
    pub contract_conflict: bool,
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
    /// Aus welchem Datensatz die Zeit stammt (nur bei zusammengefassten Konten).
    pub member: Option<String>,
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
    billable_seconds: i64,
    non_billable_seconds: i64,
    account_id: Option<String>,
    account_code: Option<String>,
    account_name: Option<String>,
}

#[derive(Debug, sqlx::FromRow)]
struct WebRow {
    id: String,
    name: String,
    short_code: String,
    contact_name: Option<String>,
    contact_email: Option<String>,
    hourly_rate: f64,
    billable_seconds: i64,
    non_billable_seconds: i64,
    site_count: i64,
    account_id: Option<String>,
    account_code: Option<String>,
    account_name: Option<String>,
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

/// Sekunden in Stunden – nur für die Anzeige, nie als Rechengrundlage.
pub fn seconds_to_hours(seconds: i64) -> f64 {
    round2(seconds as f64 / 3600.0)
}

/// Betrag für eine Dauer. Grundlage sind immer die erfassten Sekunden,
/// damit Betrag und Dauer im Nachweis zusammenpassen.
pub fn seconds_to_amount(seconds: i64, hourly_rate: f64) -> f64 {
    round2(seconds as f64 / 3600.0 * hourly_rate)
}

/// Überhang über das Inklusivvolumen, in Sekunden. Nie negativ.
pub fn overage_seconds(billable_seconds: i64, included_hours: f64) -> i64 {
    let included = (included_hours * 3600.0).round() as i64;
    (billable_seconds - included).max(0)
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
            c.account_id, a.code AS account_code, a.name AS account_name,
            COALESCE((
                SELECT SUM(te.duration_seconds) FROM time_entries te
                WHERE te.customer_id = c.id AND te.billable = true
                  AND substr(te.started_at, 1, 7) = $1
            ), 0)::bigint AS billable_seconds,
            COALESCE((
                SELECT SUM(te.duration_seconds) FROM time_entries te
                WHERE te.customer_id = c.id AND te.billable = false
                  AND substr(te.started_at, 1, 7) = $1
            ), 0)::bigint AS non_billable_seconds
        FROM customers c
        LEFT JOIN contracts ct ON ct.id = (
            SELECT id FROM contracts WHERE customer_id = c.id AND active = true LIMIT 1
        )
        LEFT JOIN accounts a ON a.id = c.account_id
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
            w.account_id, wa.code AS account_code, wa.name AS account_name,
            COALESCE((
                SELECT SUM(te.duration_seconds) FROM time_entries te
                WHERE te.web_client_id = w.id AND te.billable = true
                  AND substr(te.started_at, 1, 7) = $1
            ), 0)::bigint AS billable_seconds,
            COALESCE((
                SELECT SUM(te.duration_seconds) FROM time_entries te
                WHERE te.web_client_id = w.id AND te.billable = false
                  AND substr(te.started_at, 1, 7) = $1
            ), 0)::bigint AS non_billable_seconds,
            (SELECT COUNT(*) FROM sites s WHERE s.web_client_id = w.id) AS site_count
        FROM web_clients w
        LEFT JOIN accounts wa ON wa.id = w.account_id
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
        let idle = customer.billable_seconds <= 0
            && customer.non_billable_seconds <= 0
            && runs.is_empty();

        if idle && !include_idle {
            continue;
        }

        let rate_source = if customer.hourly_rate > 0.0 { "contract" } else { "standard" };
        let hourly_rate = if customer.hourly_rate > 0.0 {
            customer.hourly_rate
        } else {
            billing.default_hourly_rate
        };

        // Erst den gültigen Satz bestimmen, dann die Zeilen rechnen: Ohne
        // Vertrag wäre der Satz sonst 0 und jeder Einzelbetrag 0,00 €.
        let entries = load_entries(pool, Some(&customer.id), None, month, hourly_rate).await?;

        let overage_secs = overage_seconds(customer.billable_seconds, customer.included_hours);
        // Grundlage ist die Summe der Einzelbeträge – so ergibt die Addition
        // der gedruckten Zeilen genau die Kundensumme, auch auf den Cent.
        // Abgezogen wird der Wert der Inklusivstunden.
        let lines_total: f64 = entries
            .iter()
            .filter(|entry| entry.billable)
            .map(|entry| entry.amount)
            .sum();
        let included_value = round2(customer.included_hours * hourly_rate);
        let time_amount = round2((lines_total - included_value).max(0.0));
        // Auf Cent runden, bevor die Steuer darauf rechnet – sonst weicht die
        // Kundensumme um einen Cent von der Gesamtsumme ab.
        let net = round2(customer.flat_fee + time_amount);
        let (vat_amount, gross_amount) = with_vat(net, billing.vat_rate);

        let member_name = customer.name.clone();

        blocks.push(ReportBlock {
            kind: "ninjaone".to_string(),
            customer_id: customer.id,
            customer_name: customer
                .account_name
                .clone()
                .unwrap_or_else(|| customer.name.clone()),
            short_code: customer.short_code,
            contact_name: customer.contact_name,
            contact_email: customer.contact_email,
            contract_name: customer.contract_name,
            included_hours: customer.included_hours,
            hourly_rate,
            rate_source: rate_source.to_string(),
            flat_fee: customer.flat_fee,
            billable_seconds: customer.billable_seconds,
            non_billable_seconds: customer.non_billable_seconds,
            overage_seconds: overage_secs,
            billable_hours: seconds_to_hours(customer.billable_seconds),
            non_billable_hours: seconds_to_hours(customer.non_billable_seconds),
            overage_hours: seconds_to_hours(overage_secs),
            time_amount,
            total_amount: net,
            vat_amount,
            gross_amount,
            members: vec![member_name.clone()],
            account_id: customer.account_id.clone(),
            account_code: customer.account_code.clone(),
            contract_conflict: false,
            sites: Vec::new(),
            runs,
            entries: with_member(entries, &member_name),
        });
    }

    for client in web_clients {
        let sites = load_sites(pool, &client.id, month).await?;

        let action_count: usize = sites.iter().map(|site| site.actions.len()).sum();
        let idle = client.billable_seconds <= 0
            && client.non_billable_seconds <= 0
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

        let entries = load_entries(pool, None, Some(&client.id), month, hourly_rate).await?;

        let time_amount = round2(
            entries
                .iter()
                .filter(|entry| entry.billable)
                .map(|entry| entry.amount)
                .sum::<f64>(),
        );
        let (vat_amount, gross_amount) = with_vat(time_amount, billing.vat_rate);

        let member_name = client.name.clone();

        blocks.push(ReportBlock {
            kind: "web".to_string(),
            customer_id: client.id,
            customer_name: client
                .account_name
                .clone()
                .unwrap_or_else(|| client.name.clone()),
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
            billable_seconds: client.billable_seconds,
            non_billable_seconds: client.non_billable_seconds,
            overage_seconds: client.billable_seconds,
            billable_hours: seconds_to_hours(client.billable_seconds),
            non_billable_hours: seconds_to_hours(client.non_billable_seconds),
            overage_hours: seconds_to_hours(client.billable_seconds),
            time_amount,
            total_amount: time_amount,
            vat_amount,
            gross_amount,
            members: vec![member_name.clone()],
            account_id: client.account_id.clone(),
            account_code: client.account_code.clone(),
            contract_conflict: false,
            sites,
            runs: Vec::new(),
            entries: with_member(entries, &member_name),
        });
    }

    let blocks = merge_accounts(blocks, billing.vat_rate);

    let mut totals = ReportTotals {
        customers: blocks.len(),
        ..ReportTotals::default()
    };

    for block in &blocks {
        totals.billable_seconds += block.billable_seconds;
        totals.non_billable_seconds += block.non_billable_seconds;
        totals.time_amount += block.time_amount;
        totals.flat_fees += block.flat_fee;
        totals.total_amount += block.total_amount;
        totals.runs += block.runs.len();
        totals.actions += block.sites.iter().map(|site| site.actions.len()).sum::<usize>();
    }

    totals.billable_hours = seconds_to_hours(totals.billable_seconds);
    totals.non_billable_hours = seconds_to_hours(totals.non_billable_seconds);
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


/// Trägt den Namen des Datensatzes an jeder Zeile ein. Bei einem Konto mit
/// mehreren Quellen ist sonst nicht erkennbar, woher eine Zeit stammt.
fn with_member(mut entries: Vec<ReportEntry>, member: &str) -> Vec<ReportEntry> {
    for entry in &mut entries {
        entry.member = Some(member.to_string());
    }
    entries
}

/// Fasst Blöcke zusammen, die auf dasselbe Abrechnungskonto zeigen.
///
/// Ohne Konto bleibt jeder Block für sich – die Beträge ändern sich dadurch
/// nicht. Mit Konto entsteht **eine** Zeile pro Firma, in der die Zeiten,
/// Wartungsläufe und Website-Aktionen aller Datensätze zusammenlaufen.
///
/// Der Vertrag kommt aus dem Mitglied, das einen hat. Gibt es mehrere, wird
/// der erste genommen und `contract_conflict` gesetzt – die Oberfläche kann
/// dann warnen, statt still zu addieren.
fn merge_accounts(blocks: Vec<ReportBlock>, vat_rate: f64) -> Vec<ReportBlock> {
    use std::collections::BTreeMap;

    let mut groups: BTreeMap<String, Vec<ReportBlock>> = BTreeMap::new();

    for block in blocks {
        // Schlüssel: Konto, sonst der Block selbst.
        let key = match block.account_id.clone() {
            Some(account_id) => format!("account:{account_id}"),
            None => format!("source:{}:{}", block.kind, block.customer_id),
        };
        groups.entry(key).or_default().push(block);
    }

    let mut merged = Vec::with_capacity(groups.len());

    for (_, mut group) in groups {
        if group.len() == 1 {
            let block = group.remove(0);
            merged.push(block);
            continue;
        }

        // Der Datensatz mit Vertrag führt den Block an (Name, Kontakt,
        // Satz); die übrigen liefern Zeiten und Läufe.
        group.sort_by(|a, b| {
            let a_contract = a.included_hours > 0.0 || a.flat_fee > 0.0 || a.rate_source == "contract";
            let b_contract = b.included_hours > 0.0 || b.flat_fee > 0.0 || b.rate_source == "contract";
            b_contract.cmp(&a_contract)
        });

        let contracts = group
            .iter()
            .filter(|b| b.included_hours > 0.0 || b.flat_fee > 0.0 || b.rate_source == "contract")
            .count();

        let lead = &group[0];
        let included_hours = lead.included_hours;
        // Eine Rechnung, ein Satz: Der Vertragssatz der Firma gilt für alle
        // Zeiten des Kontos – auch für die, die an einem Datensatz ohne
        // eigenen Satz erfasst wurden. Sonst stünden zwei Sätze auf einem
        // Blatt, und die Summe ließe sich nicht mehr erklären.
        let hourly_rate = lead.hourly_rate;
        let flat_fee = group.iter().map(|b| b.flat_fee).sum::<f64>();
        let billable_seconds = group.iter().map(|b| b.billable_seconds).sum();
        let non_billable_seconds = group.iter().map(|b| b.non_billable_seconds).sum();
        let (vat_amount, gross_amount);

        let mut members: Vec<String> = Vec::new();
        let mut entries: Vec<ReportEntry> = Vec::new();
        let mut runs: Vec<ReportRun> = Vec::new();
        let mut sites: Vec<ReportSite> = Vec::new();

        for block in &group {
            // Die Namen der Datensätze, nicht die des Kontos: Bei einem Konto
            // heißen alle gleich, dann wäre die Herkunft nicht mehr erkennbar.
            for name in &block.members {
                if !members.contains(name) {
                    members.push(name.clone());
                }
            }
            entries.extend(block.entries.iter().map(|entry| {
                let amount = if entry.billable {
                    seconds_to_amount(entry.duration_seconds, hourly_rate)
                } else {
                    0.0
                };
                ReportEntry {
                    date: entry.date.clone(),
                    activity: entry.activity.clone(),
                    comment: entry.comment.clone(),
                    duration_seconds: entry.duration_seconds,
                    billable: entry.billable,
                    source: entry.source.clone(),
                    context: entry.context.clone(),
                    amount,
                    member: entry.member.clone(),
                }
            }));
            runs.extend(block.runs.iter().map(|run| ReportRun {
                title: run.title.clone(),
                taken_at: run.taken_at.clone(),
                status: run.status.clone(),
                assignee: run.assignee.clone(),
                tasks: run.tasks.iter().map(|task| ReportTask {
                    title: task.title.clone(),
                    target: task.target.clone(),
                    status: task.status.clone(),
                    result: task.result.clone(),
                }).collect(),
            }));
            sites.extend(block.sites.iter().map(|site| ReportSite {
                label: site.label.clone(),
                url: site.url.clone(),
                wp_version: site.wp_version.clone(),
                plugin_updates: site.plugin_updates,
                core_update: site.core_update,
                actions: site.actions.iter().map(|action| ReportAction {
                    kind: action.kind.clone(),
                    label: action.label.clone(),
                    taken_at: action.taken_at.clone(),
                    status: action.status.clone(),
                    message: action.message.clone(),
                    before_version: action.before_version.clone(),
                    after_version: action.after_version.clone(),
                }).collect(),
            }));
        }

        entries.sort_by(|a, b| a.date.cmp(&b.date));

        // Summe = Addition der gedruckten Zeilen, abzüglich des Werts des
        // Inklusivvolumens. Steuer auf den Nettobetrag.
        let lines_total: f64 = entries
            .iter()
            .filter(|entry| entry.billable)
            .map(|entry| entry.amount)
            .sum();
        let included_value = round2(included_hours * hourly_rate);
        let time_amount = round2((lines_total - included_value).max(0.0));
        let total_amount = round2(time_amount + flat_fee);
        let vat = with_vat(total_amount, vat_rate);
        vat_amount = vat.0;
        gross_amount = vat.1;
        runs.sort_by(|a, b| a.taken_at.cmp(&b.taken_at));
        sites.sort_by(|a, b| a.label.cmp(&b.label));

        merged.push(ReportBlock {
            kind: lead.kind.clone(),
            customer_id: lead.customer_id.clone(),
            customer_name: lead.customer_name.clone(),
            short_code: lead.account_code.clone().unwrap_or_else(|| lead.short_code.clone()),
            contact_name: lead.contact_name.clone(),
            contact_email: lead.contact_email.clone(),
            contract_name: lead.contract_name.clone(),
            included_hours,
            hourly_rate,
            rate_source: lead.rate_source.clone(),
            flat_fee: round2(flat_fee),
            billable_seconds,
            non_billable_seconds,
            overage_seconds: overage_seconds(billable_seconds, included_hours),
            billable_hours: seconds_to_hours(billable_seconds),
            non_billable_hours: seconds_to_hours(non_billable_seconds),
            overage_hours: seconds_to_hours(overage_seconds(billable_seconds, included_hours)),
            time_amount,
            total_amount,
            vat_amount,
            gross_amount,
            sites,
            runs,
            entries,
            account_id: lead.account_id.clone(),
            account_code: lead.account_code.clone(),
            members,
            contract_conflict: contracts > 1,
        });
    }

    merged
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
                seconds_to_amount(row.duration_seconds, hourly_rate)
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
                amount,
                member: None,
            }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::{
        monthly, overage_seconds, seconds_to_amount, seconds_to_hours, valid_month,
    };
    use crate::settings::BillingSettings;
    use crate::testdb;
    use crate::util::now_iso;
    use sqlx::PgPool;

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

    #[test]
    fn amount_matches_the_duration_that_is_printed() {
        // Der Fall aus der Praxis: 43 Sekunden angeklickt. Gedruckt wird
        // „43 s" und ein Betrag, der sich daraus nachrechnen lässt.
        assert_eq!(seconds_to_amount(43, 126.05), 1.51);
        assert_eq!(seconds_to_amount(45 * 60, 126.05), 94.54);
        assert_eq!(seconds_to_amount(0, 126.05), 0.0);

        // Wenige Sekunden ergeben einen Cent-Betrag, keine erfundene Stunde.
        assert_eq!(seconds_to_amount(5, 126.05), 0.18);
        assert_eq!(seconds_to_amount(2, 126.05), 0.07);
    }

    #[test]
    fn overage_never_goes_negative_and_respects_included_hours() {
        assert_eq!(overage_seconds(3600, 2.0), 0, "innerhalb des Inklusivvolumens");
        assert_eq!(overage_seconds(3600, 0.0), 3600);
        assert_eq!(overage_seconds(3 * 3600, 2.0), 3600);
        assert_eq!(overage_seconds(1800, 0.5), 0);
        assert_eq!(overage_seconds(1800 + 600, 0.5), 600);
    }

    #[test]
    fn hours_are_only_the_displayed_view_of_the_seconds() {
        assert_eq!(seconds_to_hours(3600), 1.0);
        assert_eq!(seconds_to_hours(45 * 60), 0.75);
        assert_eq!(seconds_to_hours(43), 0.01);
    }

    async fn customer_with(pool: &PgPool, id: &str, included: f64, rate: f64) {
        sqlx::query(
            "INSERT INTO customers (id, name, short_code, active, created_at)
             VALUES ($1, 'Testkunde', 'TST', true, $2)",
        )
        .bind(id)
        .bind(now_iso())
        .execute(pool)
        .await
        .unwrap();

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
    async fn short_entries_are_billed_by_their_real_duration() {
        let (_guard, pool) = testdb::fixture().await;
        customer_with(&pool, "k1", 0.0, 126.05).await;
        // 3 + 24 + 16 Sekunden = 43 Sekunden, so wie am 3. November erfasst.
        entry(&pool, "e1", "k1", 3).await;
        entry(&pool, "e2", "k1", 24).await;
        entry(&pool, "e3", "k1", 16).await;

        let report = monthly(&pool, "2026-11", false, &BillingSettings::default())
            .await
            .unwrap();

        let block = &report.blocks[0];
        assert_eq!(block.billable_seconds, 43);
        assert_eq!(block.overage_seconds, 43);
        assert_eq!(block.time_amount, 1.51);
        // Die gedruckten Einzelbeträge ergeben genau die Kundensumme.
        let sum: f64 = block.entries.iter().map(|entry| entry.amount).sum();
        assert_eq!((sum * 100.0).round() / 100.0, 1.51);
    }

    #[tokio::test]
    async fn report_totals_are_the_sum_of_the_customer_amounts() {
        let (_guard, pool) = testdb::fixture().await;
        customer_with(&pool, "k1", 0.0, 126.05).await;
        customer_with(&pool, "k2", 0.0, 95.0).await;
        entry(&pool, "e1", "k1", 43).await;
        entry(&pool, "e2", "k2", 2700).await;

        let report = monthly(&pool, "2026-11", false, &BillingSettings::default())
            .await
            .unwrap();

        let sum: f64 = report.blocks.iter().map(|block| block.total_amount).sum();
        assert_eq!(
            (sum * 100.0).round() / 100.0,
            report.totals.total_amount,
            "die Gesamtsumme ist die Addition der Kundensummen"
        );
        assert_eq!(report.totals.billable_seconds, 43 + 2700);
    }


    #[tokio::test]
    async fn one_account_produces_one_block() {
        let (_guard, pool) = testdb::fixture().await;
        let account = crate::accounts::create(&pool, "MWK", "Musterwerk GmbH")
            .await
            .unwrap()
            .unwrap();

        customer_with(&pool, "c1", 0.0, 100.0).await;
        sqlx::query(
            "INSERT INTO web_clients (id, name, short_code, active, created_at)
             VALUES ('w1', 'Musterwerk Web', 'MWKW', true, $1)",
        )
        .bind(now_iso())
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query("UPDATE customers SET account_id = $1 WHERE id = 'c1'")
            .bind(&account.id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("UPDATE web_clients SET account_id = $1 WHERE id = 'w1'")
            .bind(&account.id)
            .execute(&pool)
            .await
            .unwrap();

        entry(&pool, "e1", "c1", 3600).await;
        sqlx::query(
            "INSERT INTO time_entries
                (id, web_client_id, user_name, activity, started_at, ended_at,
                 duration_seconds, billable, source, created_at)
             VALUES ('e2', 'w1', 'S. Wolf', 'Webseite', '2026-11-03T10:00:00',
                     '2026-11-03T11:00:00', 1800, true, 'timer', $1)",
        )
        .bind(now_iso())
        .execute(&pool)
        .await
        .unwrap();

        let report = monthly(&pool, "2026-11", false, &BillingSettings::default())
            .await
            .unwrap();

        assert_eq!(report.blocks.len(), 1, "eine Firma, ein Block");
        let block = &report.blocks[0];
        assert_eq!(block.account_code.as_deref(), Some("MWK"));
        assert_eq!(block.billable_seconds, 5400);
        assert_eq!(block.entries.len(), 2);
        assert_eq!(block.members.len(), 2, "beide Quellen sind benannt");
        assert_eq!(block.time_amount, 150.0);
        // Die Zeilenbeträge ergeben weiterhin die Kundensumme.
        let sum: f64 = block.entries.iter().map(|e| e.amount).sum();
        assert_eq!((sum * 100.0).round() / 100.0, block.time_amount);
    }

    #[tokio::test]
    async fn entry_amounts_use_the_standard_rate_without_a_contract() {
        let (_guard, pool) = testdb::fixture().await;
        // Kein Vertrag: der Satz kommt aus den Einstellungen, nicht aus 0.
        sqlx::query(
            "INSERT INTO customers (id, name, short_code, active, created_at)
             VALUES ('k1', 'Ohne Vertrag', 'OHV', true, $1)",
        )
        .bind(now_iso())
        .execute(&pool)
        .await
        .unwrap();
        entry(&pool, "e1", "k1", 5400).await;

        let billing = BillingSettings {
            default_hourly_rate: 100.0,
            vat_rate: 19.0,
        };
        let report = monthly(&pool, "2026-11", false, &billing).await.unwrap();

        let block = &report.blocks[0];
        assert_eq!(block.rate_source, "standard");
        assert_eq!(block.entries[0].amount, 150.0);
        assert_eq!(block.time_amount, 150.0);
    }

    #[tokio::test]
    async fn customer_amount_is_the_sum_of_the_printed_lines() {
        let (_guard, pool) = testdb::fixture().await;
        customer_with(&pool, "k1", 0.0, 126.05).await;
        // 3 + 24 + 16 + 5400 Sekunden: einzeln gerundet ergibt 190,59 €.
        entry(&pool, "e1", "k1", 3).await;
        entry(&pool, "e2", "k1", 24).await;
        entry(&pool, "e3", "k1", 16).await;
        entry(&pool, "e4", "k1", 5400).await;

        let report = monthly(&pool, "2026-11", false, &BillingSettings::default())
            .await
            .unwrap();

        let block = &report.blocks[0];
        let sum: f64 = block.entries.iter().map(|entry| entry.amount).sum();
        assert_eq!(
            (sum * 100.0).round() / 100.0,
            block.time_amount,
            "die Kundensumme ist die Addition der Zeilenbeträge"
        );
    }

    #[tokio::test]
    async fn included_hours_are_deducted_in_seconds() {
        let (_guard, pool) = testdb::fixture().await;
        customer_with(&pool, "k1", 2.0, 100.0).await;
        // 1 Stunde 59 Minuten 30 Sekunden: noch im Inklusivvolumen.
        entry(&pool, "e1", "k1", 7170).await;

        let report = monthly(&pool, "2026-11", false, &BillingSettings::default())
            .await
            .unwrap();

        assert_eq!(report.blocks[0].overage_seconds, 0);
        assert_eq!(report.blocks[0].time_amount, 0.0);
    }
}
