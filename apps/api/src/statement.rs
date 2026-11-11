//! Aufstellung offener Leistungen eines Kunden oder Abrechnungskontos.
//!
//! Anders als der Monatsbericht ist die Aufstellung nicht an einen Monat
//! gebunden: Sie sammelt alle Zeiten, die noch nicht als abgerechnet markiert
//! sind (`time_entries.billed_at IS NULL`). Damit lässt sich ein Kunde
//! abrechnen, auch wenn der Zeitraum über einen Monatswechsel läuft – und
//! nach dem Druck wird dieselbe Liste als abgerechnet markiert, sodass sie
//! beim nächsten Mal nicht wieder auftaucht.
//!
//! Gehört ein Kunde zu einem Abrechnungskonto, enthält die Aufstellung **alle**
//! offenen Zeiten dieses Kontos – eine Firma bekommt eine Rechnung, auch wenn
//! ihre Arbeit über mehrere Datensätze verteilt erfasst wurde.
//!
//! Der laufende Timer bleibt außen vor (`ended_at IS NULL`): Er hat noch
//! keine endgültige Dauer.

use serde::Serialize;
use sqlx::PgPool;

use crate::billing::with_vat;
use crate::reports::{seconds_to_amount, seconds_to_hours};
use crate::settings::BillingSettings;
use crate::util::now_iso;

#[derive(Debug, Serialize)]
pub struct Statement {
    /// `customer`, `web` oder `account`.
    pub kind: String,
    pub owner_id: String,
    pub name: String,
    pub short_code: String,
    /// Konto, über das die Aufstellung läuft – `None`, wenn keins gesetzt ist.
    pub account_id: Option<String>,
    pub account_code: Option<String>,
    /// Alle Datensätze, aus denen die Zeiten stammen, mit Namen.
    pub members: Vec<StatementMember>,
    /// Mehr als ein Vertrag im Konto – sollte nicht vorkommen, wird gemeldet.
    pub contract_conflict: bool,
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

#[derive(Debug, Serialize, Clone)]
pub struct StatementMember {
    /// `customer` oder `web`.
    pub kind: String,
    pub id: String,
    pub name: String,
}

#[derive(Debug, Serialize)]
pub struct StatementEntry {
    pub id: String,
    pub date: String,
    pub activity: String,
    pub comment: Option<String>,
    pub context: Option<String>,
    pub user_name: String,
    /// Aus welchem Datensatz die Zeit stammt (nur bei Konten mit mehreren).
    pub member: Option<String>,
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

/// Kopf der Aufstellung: wer bekommt die Rechnung.
struct Owner {
    id: String,
    name: String,
    short_code: String,
    contact_name: Option<String>,
    contact_email: Option<String>,
    phone: Option<String>,
    contract_name: Option<String>,
    included_hours: f64,
    hourly_rate: f64,
    contract_conflict: bool,
    account_id: Option<String>,
    account_code: Option<String>,
    /// `contract` oder `standard` – wird beim Aufbau ermittelt.
    rate_source: String,
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
    member_name: Option<String>,
}

fn round2(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

/// Aufstellung für einen NinjaOne-Kunden.
///
/// Gehört der Kunde zu einem Abrechnungskonto, wird die Aufstellung des
/// **Kontos** geliefert – die Rechnung gilt der Firma, nicht dem Datensatz.
pub async fn for_customer(
    pool: &PgPool,
    customer_id: &str,
    billing: &BillingSettings,
) -> Result<Option<Statement>, sqlx::Error> {
    let account: Option<(String, String)> = sqlx::query_as(
        "SELECT a.id, a.code FROM customers c
         JOIN accounts a ON a.id = c.account_id
         WHERE c.id = $1",
    )
    .bind(customer_id)
    .fetch_optional(pool)
    .await?;

    if let Some((account_id, _code)) = account {
        return for_account(pool, &account_id, billing).await;
    }

    let owner = sqlx::query_as::<_, (String, String, String, Option<String>, Option<String>, Option<String>, Option<String>, f64, f64)>(
        r#"
        SELECT
            c.id, c.name, c.short_code, c.contact_name, c.contact_email, c.phone,
            ct.name,
            CAST(COALESCE(ct.included_hours, 0) AS DOUBLE PRECISION),
            CAST(COALESCE(ct.hourly_rate, 0) AS DOUBLE PRECISION)
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

    let Some((id, name, short_code, contact_name, contact_email, phone, contract_name, included, rate)) = owner else {
        return Ok(None);
    };

    let rows = sqlx::query_as::<_, EntryRow>(&format!(
        "{OPEN_ENTRIES} AND te.customer_id = $1 ORDER BY te.started_at"
    ))
    .bind(customer_id)
    .fetch_all(pool)
    .await?;

    let owner = Owner {
        id: id.clone(),
        name,
        short_code,
        contact_name,
        contact_email,
        phone,
        contract_name,
        included_hours: included,
        hourly_rate: rate,
        contract_conflict: false,
        account_id: None,
        account_code: None,
        rate_source: if rate > 0.0 { "contract" } else { "standard" }.to_string(),
    };

    Ok(Some(build(
        "customer",
        owner,
        vec![StatementMember {
            kind: "customer".to_string(),
            id,
            name: String::new(),
        }],
        rows,
        billing,
    )))
}

/// Aufstellung für einen Web-Kunden; bei Konto ebenfalls die des Kontos.
pub async fn for_web_client(
    pool: &PgPool,
    web_client_id: &str,
    billing: &BillingSettings,
) -> Result<Option<Statement>, sqlx::Error> {
    let account: Option<(String, String)> = sqlx::query_as(
        "SELECT a.id, a.code FROM web_clients w
         JOIN accounts a ON a.id = w.account_id
         WHERE w.id = $1",
    )
    .bind(web_client_id)
    .fetch_optional(pool)
    .await?;

    if let Some((account_id, _code)) = account {
        return for_account(pool, &account_id, billing).await;
    }

    let owner = sqlx::query_as::<_, (String, String, String, Option<String>, Option<String>, Option<String>, f64)>(
        r#"
        SELECT w.id, w.name, w.short_code, w.contact_name, w.contact_email, w.phone,
               CAST(w.hourly_rate AS DOUBLE PRECISION)
        FROM web_clients w
        WHERE w.id = $1
        "#,
    )
    .bind(web_client_id)
    .fetch_optional(pool)
    .await?;

    let Some((id, name, short_code, contact_name, contact_email, phone, rate)) = owner else {
        return Ok(None);
    };

    let rows = sqlx::query_as::<_, EntryRow>(&format!(
        "{OPEN_ENTRIES} AND te.web_client_id = $1 ORDER BY te.started_at"
    ))
    .bind(web_client_id)
    .fetch_all(pool)
    .await?;

    let owner = Owner {
        id: id.clone(),
        name: name.clone(),
        short_code,
        contact_name,
        contact_email,
        phone,
        contract_name: None,
        included_hours: 0.0,
        hourly_rate: rate,
        contract_conflict: false,
        account_id: None,
        account_code: None,
        rate_source: if rate > 0.0 { "contract" } else { "standard" }.to_string(),
    };

    Ok(Some(build(
        "web",
        owner,
        vec![StatementMember {
            kind: "web".to_string(),
            id,
            name,
        }],
        rows,
        billing,
    )))
}

/// Aufstellung für ein Abrechnungskonto: alle offenen Zeiten aller Mitglieder.
pub async fn for_account(
    pool: &PgPool,
    account_id: &str,
    billing: &BillingSettings,
) -> Result<Option<Statement>, sqlx::Error> {
    let account: Option<(String, String, String)> =
        sqlx::query_as("SELECT id, code, name FROM accounts WHERE id = $1")
            .bind(account_id)
            .fetch_optional(pool)
            .await?;

    let Some((id, code, name)) = account else {
        return Ok(None);
    };

    let customer_members: Vec<(String, String, Option<String>, Option<String>, Option<String>)> =
        sqlx::query_as(
            "SELECT id, name, contact_name, contact_email, phone
             FROM customers WHERE account_id = $1 ORDER BY name",
        )
        .bind(account_id)
        .fetch_all(pool)
        .await?;

    let web_members: Vec<(String, String, Option<String>, Option<String>, Option<String>)> =
        sqlx::query_as(
            "SELECT id, name, contact_name, contact_email, phone
             FROM web_clients WHERE account_id = $1 ORDER BY name",
        )
        .bind(account_id)
        .fetch_all(pool)
        .await?;

    let customer_ids: Vec<String> = customer_members.iter().map(|m| m.0.clone()).collect();
    let web_ids: Vec<String> = web_members.iter().map(|m| m.0.clone()).collect();

    // Ein Wartungsvertrag pro Firma. Mehrere wären ein Datenfehler, den die
    // Aufstellung nicht stillschweigend auflösen darf.
    let contracts: Vec<(String, Option<String>, f64, f64)> = sqlx::query_as(
        "SELECT c.id, ct.name,
                CAST(COALESCE(ct.included_hours, 0) AS DOUBLE PRECISION),
                CAST(COALESCE(ct.hourly_rate, 0) AS DOUBLE PRECISION)
         FROM customers c
         JOIN contracts ct ON ct.customer_id = c.id AND ct.active = true
         WHERE c.account_id = $1
         ORDER BY c.name, ct.name",
    )
    .bind(account_id)
    .fetch_all(pool)
    .await?;

    let contract_conflict = contracts.len() > 1;
    let contract = contracts.first();
    let included_hours = contract.map(|c| c.2).unwrap_or(0.0);
    let contract_rate = contract.map(|c| c.3).unwrap_or(0.0);
    let contract_name = contract.and_then(|c| c.1.clone());

    // Satz: Vertrag, sonst Satz eines Web-Kunden im Konto, sonst Standard.
    let web_rate: Option<f64> = sqlx::query_scalar(
        "SELECT hourly_rate FROM web_clients WHERE account_id = $1 AND hourly_rate > 0 LIMIT 1",
    )
    .bind(account_id)
    .fetch_optional(pool)
    .await?;

    let hourly_rate = if contract_rate > 0.0 {
        contract_rate
    } else {
        web_rate.unwrap_or(billing.default_hourly_rate)
    };

    let rows = sqlx::query_as::<_, EntryRow>(&format!(
        "{OPEN_ENTRIES}
         AND (te.customer_id = ANY($1) OR te.web_client_id = ANY($2))
         ORDER BY te.started_at"
    ))
    .bind(&customer_ids)
    .bind(&web_ids)
    .fetch_all(pool)
    .await?;

    // Ansprechpartner: der erste Datensatz mit Kontaktdaten. Vor dem Umsetzen
    // der Listen ermitteln, weil `members` sie unten übernimmt.
    let contact = customer_members
        .iter()
        .chain(web_members.iter())
        .find(|m| m.2.is_some() || m.3.is_some())
        .map(|m| (m.2.clone(), m.3.clone(), m.4.clone()));

    let mut members: Vec<StatementMember> = customer_members
        .into_iter()
        .map(|(id, name, _, _, _)| StatementMember {
            kind: "customer".to_string(),
            id,
            name,
        })
        .collect();
    members.extend(web_members.into_iter().map(|(id, name, _, _, _)| StatementMember {
        kind: "web".to_string(),
        id,
        name,
    }));

    let rate_source = if contract_rate > 0.0 {
        "contract"
    } else {
        "standard"
    };

    let owner = Owner {
        id: id.clone(),
        name,
        short_code: code.clone(),
        contact_name: contact.clone().and_then(|c| c.0),
        contact_email: contact.clone().and_then(|c| c.1),
        phone: contact.and_then(|c| c.2),
        contract_name,
        included_hours,
        hourly_rate,
        contract_conflict,
        account_id: Some(id),
        account_code: Some(code),
        rate_source: rate_source.to_string(),
    };

    Ok(Some(build("account", owner, members, rows, billing)))
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
        t.title AS task_title,
        COALESCE(c.name, w.name) AS member_name
    FROM time_entries te
    LEFT JOIN customers c ON c.id = te.customer_id
    LEFT JOIN web_clients w ON w.id = te.web_client_id
    LEFT JOIN sites s ON s.id = te.site_id
    LEFT JOIN devices d ON d.id = te.device_id
    LEFT JOIN tasks t ON t.id = te.task_id
    WHERE te.billed_at IS NULL AND te.ended_at IS NOT NULL
"#;

fn build(
    kind: &str,
    owner: Owner,
    members: Vec<StatementMember>,
    rows: Vec<EntryRow>,
    billing: &BillingSettings,
) -> Statement {
    let rate_source = owner.rate_source.clone();

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
            seconds_to_amount(row.duration_seconds, hourly_rate)
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
            member: row.member_name,
            duration_seconds: row.duration_seconds,
            billable: row.billable,
            amount,
        });
    }

    let first_date = entries.first().map(|entry| entry.date.clone());
    let last_date = entries.last().map(|entry| entry.date.clone());

    // Einzelbeträge sind gerundet; die Summe muss ihre Addition sein.
    totals.entries = entries.len();
    totals.billable_hours = seconds_to_hours(totals.billable_seconds);
    totals.non_billable_hours = seconds_to_hours(totals.non_billable_seconds);
    totals.time_amount = round2(totals.time_amount);

    let (vat_amount, gross_amount) = with_vat(totals.time_amount, billing.vat_rate);
    totals.vat_amount = vat_amount;
    totals.gross_amount = gross_amount;

    Statement {
        kind: kind.to_string(),
        owner_id: owner.id,
        name: owner.name,
        short_code: owner.short_code,
        account_id: owner.account_id,
        account_code: owner.account_code,
        members,
        contract_conflict: owner.contract_conflict,
        contact_name: owner.contact_name,
        contact_email: owner.contact_email,
        phone: owner.phone,
        contract_name: owner.contract_name,
        included_hours: owner.included_hours,
        hourly_rate,
        rate_source,
        generated_at: now_iso(),
        generated_by: String::new(),
        first_date,
        last_date,
        entries,
        totals,
    }
}

/// Markiert die übergebenen Einträge eines Kunden als abgerechnet.
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

/// Markiert Zeiten eines Kontos. Trifft nur Einträge, die zu einem Mitglied
/// des Kontos gehören – geprüft in der Datenbank, nicht nur in der Oberfläche.
pub async fn mark_billed_account(
    pool: &PgPool,
    account_id: &str,
    entry_ids: &[String],
) -> Result<u64, sqlx::Error> {
    if entry_ids.is_empty() {
        return Ok(0);
    }

    let (customers, web_clients) = crate::accounts::members(pool, account_id).await?;

    let result = sqlx::query(
        "UPDATE time_entries SET billed_at = $1
         WHERE billed_at IS NULL
           AND id = ANY($2)
           AND (customer_id = ANY($3) OR web_client_id = ANY($4))",
    )
    .bind(now_iso())
    .bind(entry_ids)
    .bind(&customers)
    .bind(&web_clients)
    .execute(pool)
    .await?;

    Ok(result.rows_affected())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::accounts;
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

    async fn web_client(pool: &PgPool, id: &str, name: &str) {
        sqlx::query(
            "INSERT INTO web_clients (id, name, short_code, active, created_at)
             VALUES ($1, $2, 'WEB', true, $3)",
        )
        .bind(id)
        .bind(name)
        .bind(now_iso())
        .execute(pool)
        .await
        .unwrap();
    }

    async fn entry(
        pool: &PgPool,
        id: &str,
        customer_id: Option<&str>,
        web_client_id: Option<&str>,
        seconds: i64,
        billable: bool,
        billed_at: Option<&str>,
    ) {
        sqlx::query(
            "INSERT INTO time_entries
                (id, customer_id, web_client_id, user_name, activity, started_at, ended_at,
                 duration_seconds, billable, source, created_at, billed_at)
             VALUES ($1, $2, $3, 'S. Wolf', 'Fernwartung', '2026-10-05T09:00:00',
                     '2026-10-05T10:00:00', $4, $5, 'timer', '2026-10-05T10:00:00', $6)",
        )
        .bind(id)
        .bind(customer_id)
        .bind(web_client_id)
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
        entry(&pool, "e1", Some("k1"), None, 3600, true, None).await;
        entry(&pool, "e2", Some("k1"), None, 1800, true, Some("2026-10-01T00:00:00")).await;
        entry(&pool, "e3", Some("k1"), None, 900, false, None).await;

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
        entry(&pool, "e1", Some("k2"), None, 7200, true, None).await;

        let billing = BillingSettings {
            default_hourly_rate: 90.0,
            vat_rate: 19.0,
        };
        let statement = for_customer(&pool, "k2", &billing).await.unwrap().unwrap();

        assert_eq!(statement.hourly_rate, 90.0);
        assert_eq!(statement.totals.time_amount, 180.0);
        assert_eq!(statement.totals.gross_amount, 214.2);
    }

    #[tokio::test]
    async fn entries_are_listed_chronologically() {
        let (_guard, pool) = testdb::fixture().await;
        customer(&pool, "k7", "Zeiträume", 100.0).await;

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
    }

    #[tokio::test]
    async fn marking_billed_clears_the_statement_and_is_repeatable() {
        let (_guard, pool) = testdb::fixture().await;
        customer(&pool, "k3", "Kunde Drei", 100.0).await;
        entry(&pool, "e1", Some("k3"), None, 3600, true, None).await;
        entry(&pool, "e2", Some("k3"), None, 600, true, None).await;

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
    }

    #[tokio::test]
    async fn marking_billed_does_not_touch_other_customers() {
        let (_guard, pool) = testdb::fixture().await;
        customer(&pool, "k4", "Kunde Vier", 100.0).await;
        customer(&pool, "k5", "Kunde Fünf", 100.0).await;
        entry(&pool, "e1", Some("k4"), None, 3600, true, None).await;
        entry(&pool, "e2", Some("k5"), None, 3600, true, None).await;

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

    // -----------------------------------------------------------------------
    // Abrechnungskonten
    // -----------------------------------------------------------------------

    /// Dieselbe Firma als NinjaOne-Kunde und als Web-Kunde: eine Rechnung.
    #[tokio::test]
    async fn account_gathers_entries_from_all_members() {
        let (_guard, pool) = testdb::fixture().await;
        let account = accounts::create(&pool, "MWK", "Musterwerk GmbH")
            .await
            .unwrap()
            .unwrap();

        customer(&pool, "c1", "Musterwerk GmbH", 120.0).await;
        web_client(&pool, "w1", "Musterwerk Web").await;
        accounts::assign_customer(&pool, "c1", Some(&account.id)).await.unwrap();
        accounts::assign_web_client(&pool, "w1", Some(&account.id)).await.unwrap();

        entry(&pool, "e1", Some("c1"), None, 3600, true, None).await;
        entry(&pool, "e2", None, Some("w1"), 1800, true, None).await;

        // Der Web-Kunde fragt ab – bekommt aber die Rechnung des Kontos.
        let statement = for_web_client(&pool, "w1", &BillingSettings::default())
            .await
            .unwrap()
            .unwrap();

        assert_eq!(statement.kind, "account");
        assert_eq!(statement.account_code.as_deref(), Some("MWK"));
        assert_eq!(statement.entries.len(), 2);
        assert_eq!(statement.totals.billable_seconds, 5400);
        // 1,5 h × 120 € = 180 €, einmal für die Firma.
        assert_eq!(statement.totals.time_amount, 180.0);
    }

    #[tokio::test]
    async fn account_statement_names_the_source_of_each_entry() {
        let (_guard, pool) = testdb::fixture().await;
        let account = accounts::create(&pool, "MWK", "Musterwerk GmbH")
            .await
            .unwrap()
            .unwrap();

        customer(&pool, "c1", "Musterwerk GmbH", 100.0).await;
        web_client(&pool, "w1", "Musterwerk Web").await;
        accounts::assign_customer(&pool, "c1", Some(&account.id)).await.unwrap();
        accounts::assign_web_client(&pool, "w1", Some(&account.id)).await.unwrap();

        entry(&pool, "e1", Some("c1"), None, 600, true, None).await;
        entry(&pool, "e2", None, Some("w1"), 600, true, None).await;

        let statement = for_account(&pool, &account.id, &BillingSettings::default())
            .await
            .unwrap()
            .unwrap();

        let members: Vec<&str> = statement
            .entries
            .iter()
            .filter_map(|e| e.member.as_deref())
            .collect();
        assert!(members.contains(&"Musterwerk GmbH"), "{members:?}");
        assert!(members.contains(&"Musterwerk Web"), "{members:?}");
    }

    #[tokio::test]
    async fn account_uses_the_single_contract_only_once() {
        let (_guard, pool) = testdb::fixture().await;
        let account = accounts::create(&pool, "MWK", "Musterwerk GmbH")
            .await
            .unwrap()
            .unwrap();

        customer(&pool, "c1", "Musterwerk GmbH", 100.0).await;
        sqlx::query("UPDATE contracts SET included_hours = 2 WHERE customer_id = 'c1'")
            .execute(&pool)
            .await
            .unwrap();
        web_client(&pool, "w1", "Musterwerk Web").await;
        accounts::assign_customer(&pool, "c1", Some(&account.id)).await.unwrap();
        accounts::assign_web_client(&pool, "w1", Some(&account.id)).await.unwrap();

        let statement = for_account(&pool, &account.id, &BillingSettings::default())
            .await
            .unwrap()
            .unwrap();

        // Das Inklusivvolumen steht einmal da, nicht je Datensatz.
        assert_eq!(statement.included_hours, 2.0);
        assert!(!statement.contract_conflict);
        assert_eq!(statement.hourly_rate, 100.0);
    }

    #[tokio::test]
    async fn two_contracts_in_one_account_are_reported() {
        let (_guard, pool) = testdb::fixture().await;
        let account = accounts::create(&pool, "MWK", "Musterwerk GmbH")
            .await
            .unwrap()
            .unwrap();

        customer(&pool, "c1", "Musterwerk GmbH", 100.0).await;
        customer(&pool, "c2", "Musterwerk Süd", 120.0).await;
        accounts::assign_customer(&pool, "c1", Some(&account.id)).await.unwrap();
        accounts::assign_customer(&pool, "c2", Some(&account.id)).await.unwrap();

        let statement = for_account(&pool, &account.id, &BillingSettings::default())
            .await
            .unwrap()
            .unwrap();

        assert!(statement.contract_conflict, "mehrere Verträge müssen auffallen");
    }

    #[tokio::test]
    async fn marking_an_account_bills_all_its_members() {
        let (_guard, pool) = testdb::fixture().await;
        let account = accounts::create(&pool, "MWK", "Musterwerk GmbH")
            .await
            .unwrap()
            .unwrap();

        customer(&pool, "c1", "Musterwerk GmbH", 100.0).await;
        web_client(&pool, "w1", "Musterwerk Web").await;
        accounts::assign_customer(&pool, "c1", Some(&account.id)).await.unwrap();
        accounts::assign_web_client(&pool, "w1", Some(&account.id)).await.unwrap();

        entry(&pool, "e1", Some("c1"), None, 600, true, None).await;
        entry(&pool, "e2", None, Some("w1"), 600, true, None).await;

        let ids = vec!["e1".to_string(), "e2".to_string()];
        assert_eq!(mark_billed_account(&pool, &account.id, &ids).await.unwrap(), 2);

        let after = for_account(&pool, &account.id, &BillingSettings::default())
            .await
            .unwrap()
            .unwrap();
        assert!(after.entries.is_empty());
    }

    #[tokio::test]
    async fn marking_an_account_ignores_entries_of_other_accounts() {
        let (_guard, pool) = testdb::fixture().await;
        let ahs = accounts::create(&pool, "MWK", "Musterwerk GmbH").await.unwrap().unwrap();
        let other = accounts::create(&pool, "BSP", "Beispiel GmbH").await.unwrap().unwrap();

        customer(&pool, "c1", "Musterwerk GmbH", 100.0).await;
        customer(&pool, "c2", "Beispiel GmbH", 100.0).await;
        accounts::assign_customer(&pool, "c1", Some(&ahs.id)).await.unwrap();
        accounts::assign_customer(&pool, "c2", Some(&other.id)).await.unwrap();

        entry(&pool, "e1", Some("c1"), None, 600, true, None).await;
        entry(&pool, "e2", Some("c2"), None, 600, true, None).await;

        let marked = mark_billed_account(
            &pool,
            &ahs.id,
            &["e1".to_string(), "e2".to_string()],
        )
        .await
        .unwrap();

        assert_eq!(marked, 1, "fremde Einträge bleiben offen");
        let other_statement = for_account(&pool, &other.id, &BillingSettings::default())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(other_statement.entries.len(), 1);
    }

    #[tokio::test]
    async fn customer_without_account_behaves_as_before() {
        let (_guard, pool) = testdb::fixture().await;
        customer(&pool, "k9", "Einzelkunde", 100.0).await;
        entry(&pool, "e1", Some("k9"), None, 3600, true, None).await;

        let statement = for_customer(&pool, "k9", &BillingSettings::default())
            .await
            .unwrap()
            .unwrap();

        assert_eq!(statement.kind, "customer");
        assert!(statement.account_id.is_none());
        assert_eq!(statement.entries.len(), 1);
    }
}
