//! Abrechnungskonten.
//!
//! Ein Konto ist die Firma, die eine Rechnung bekommt. Im Cockpit kann
//! dieselbe Firma mehrfach vorkommen – als NinjaOne-Kunde, als Web-Kunde und
//! (künftig) als TeamViewer-Kunde. Alle Datensätze, die auf dasselbe Konto
//! zeigen, werden in Bericht, Aufstellung und Abrechnungsübersicht zu **einer**
//! Zeile zusammengefasst.
//!
//! Ohne Konto bleibt alles wie vorher: Der Datensatz rechnet für sich selbst.
//! So lassen sich Konten schrittweise einführen, ohne bestehende Beträge zu
//! verändern.

use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use crate::util::now_iso;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Account {
    pub id: String,
    pub code: String,
    pub name: String,
    pub active: bool,
    /// Wie viele Datensätze auf dieses Konto zeigen.
    pub customer_count: i64,
    pub web_client_count: i64,
}

#[derive(Debug, Deserialize)]
pub struct CreateAccountRequest {
    pub code: String,
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateAccountRequest {
    pub code: Option<String>,
    pub name: Option<String>,
    pub active: Option<bool>,
}

/// Kürzel normalisieren: „mwk" und „ MWK " sind dasselbe Konto.
pub fn normalize_code(code: &str) -> String {
    code.trim().to_uppercase()
}

/// Alle Konten mit der Zahl der zugeordneten Datensätze.
pub async fn list(pool: &PgPool) -> Result<Vec<Account>, sqlx::Error> {
    sqlx::query_as::<_, Account>(
        r#"
        SELECT
            a.id,
            a.code,
            a.name,
            a.active,
            (SELECT COUNT(*) FROM customers c WHERE c.account_id = a.id) AS customer_count,
            (SELECT COUNT(*) FROM web_clients w WHERE w.account_id = a.id) AS web_client_count
        FROM accounts a
        ORDER BY a.code
        "#,
    )
    .fetch_all(pool)
    .await
}

/// Ein einzelnes Konto.
pub async fn get(pool: &PgPool, id: &str) -> Result<Option<Account>, sqlx::Error> {
    sqlx::query_as::<_, Account>(
        r#"
        SELECT
            a.id,
            a.code,
            a.name,
            a.active,
            (SELECT COUNT(*) FROM customers c WHERE c.account_id = a.id) AS customer_count,
            (SELECT COUNT(*) FROM web_clients w WHERE w.account_id = a.id) AS web_client_count
        FROM accounts a
        WHERE a.id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await
}

/// Legt ein Konto an. Gibt `None` zurück, wenn das Kürzel schon vergeben ist.
pub async fn create(
    pool: &PgPool,
    code: &str,
    name: &str,
) -> Result<Option<Account>, sqlx::Error> {
    let code = normalize_code(code);
    let id = uuid::Uuid::new_v4().to_string();

    let inserted = sqlx::query(
        "INSERT INTO accounts (id, code, name, active, created_at)
         VALUES ($1, $2, $3, true, $4)
         ON CONFLICT DO NOTHING",
    )
    .bind(&id)
    .bind(&code)
    .bind(name.trim())
    .bind(now_iso())
    .execute(pool)
    .await?;

    if inserted.rows_affected() == 0 {
        return Ok(None);
    }

    get(pool, &id).await
}

/// Ändert Kürzel, Namen oder Aktivierung. `Ok(None)` heißt: Kürzel vergeben.
pub async fn update(
    pool: &PgPool,
    id: &str,
    code: Option<&str>,
    name: Option<&str>,
    active: Option<bool>,
) -> Result<Option<Account>, sqlx::Error> {
    if let Some(code) = code {
        let code = normalize_code(code);
        let taken: Option<String> =
            sqlx::query_scalar("SELECT id FROM accounts WHERE lower(code) = lower($1) AND id <> $2")
                .bind(&code)
                .bind(id)
                .fetch_optional(pool)
                .await?;

        if taken.is_some() {
            return Ok(None);
        }

        sqlx::query("UPDATE accounts SET code = $1 WHERE id = $2")
            .bind(&code)
            .bind(id)
            .execute(pool)
            .await?;
    }

    if let Some(name) = name {
        sqlx::query("UPDATE accounts SET name = $1 WHERE id = $2")
            .bind(name.trim())
            .bind(id)
            .execute(pool)
            .await?;
    }

    if let Some(active) = active {
        sqlx::query("UPDATE accounts SET active = $1 WHERE id = $2")
            .bind(active)
            .bind(id)
            .execute(pool)
            .await?;
    }

    get(pool, id).await
}

/// Weist einen Kunden einem Konto zu. `None` löst die Zuordnung.
///
/// Die Zuordnung liegt am Kunden, nicht am Konto – so bleibt die Änderung
/// dort, wo der Benutzer sie erwartet, und ein Konto kann nicht versehentlich
/// fremde Kunden mitnehmen.
pub async fn assign_customer(
    pool: &PgPool,
    customer_id: &str,
    account_id: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE customers SET account_id = $1 WHERE id = $2")
        .bind(account_id)
        .bind(customer_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Weist einen Web-Kunden einem Konto zu.
pub async fn assign_web_client(
    pool: &PgPool,
    web_client_id: &str,
    account_id: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE web_clients SET account_id = $1 WHERE id = $2")
        .bind(account_id)
        .bind(web_client_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Mitglieder eines Kontos: NinjaOne-Kunden und Web-Kunden getrennt.
///
/// Wird beim Markieren als abgerechnet gebraucht – ein Konto umfasst mehrere
/// Datensätze, und alle offenen Zeiten des Kontos sollen mit einem Schritt
/// abgehakt werden.
pub async fn members(
    pool: &PgPool,
    account_id: &str,
) -> Result<(Vec<String>, Vec<String>), sqlx::Error> {
    let customers: Vec<String> =
        sqlx::query_scalar("SELECT id FROM customers WHERE account_id = $1")
            .bind(account_id)
            .fetch_all(pool)
            .await?;

    let web_clients: Vec<String> =
        sqlx::query_scalar("SELECT id FROM web_clients WHERE account_id = $1")
            .bind(account_id)
            .fetch_all(pool)
            .await?;

    Ok((customers, web_clients))
}

// ---------------------------------------------------------------------------
// Zuordnung von der Kontoseite aus
// ---------------------------------------------------------------------------

/// Ein Konto mit seinen zugeordneten Datensätzen – für die Übersicht in den
/// Einstellungen, wo die Zuordnung an **einer** Stelle gepflegt wird.
#[derive(Debug, Serialize)]
pub struct AccountAssignmentRow {
    pub account_id: String,
    pub code: String,
    pub name: String,
    pub active: bool,
    /// Zuordnung aus der einen Quelle (NinjaOne-Kunde).
    pub customer_id: Option<String>,
    pub customer_name: Option<String>,
    /// Zuordnung aus der anderen Quelle (Web-Kunde).
    pub web_client_id: Option<String>,
    pub web_client_name: Option<String>,
    /// TeamViewer-Gruppen, die auf dieses Konto zeigen.
    pub teamviewer_groups: Vec<TeamViewerAssignment>,
}

#[derive(Debug, Serialize)]
pub struct TeamViewerAssignment {
    pub group_id: String,
    pub name: String,
}

/// Eine Auswahlmöglichkeit in den Klapplisten.
#[derive(Debug, Serialize)]
pub struct AssignmentTarget {
    pub id: String,
    pub name: String,
    /// Bereits einem anderen Konto zugeordnet – wird im Menü gekennzeichnet.
    pub taken_by: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct AccountOverview {
    pub accounts: Vec<AccountAssignmentRow>,
    /// Kunden, die noch keinem Konto zugeordnet sind ODER diesem zugeordnet
    /// sind. Bereits vergebene erscheinen mit `taken_by`, damit man sieht,
    /// warum ein Wechsel das andere Konto leert.
    pub customers: Vec<AssignmentTarget>,
    pub web_clients: Vec<AssignmentTarget>,
    pub teamviewer_groups: Vec<AssignmentTarget>,
    /// Datensätze ohne Konto – hier beginnt die Arbeit.
    pub unassigned_customers: usize,
    pub unassigned_teamviewer: usize,
}

/// Baut die Übersicht für die Einstellungen.
///
/// Die Konten führen; je Konto stehen die zugeordneten Datensätze. Die Listen
/// enthalten **alle** Datensätze mit dem Hinweis, welchem Konto sie gehören –
/// so ist ein Umhängen möglich, ohne dass eine Zeile aus dem Menü verschwindet.
pub async fn overview(pool: &PgPool) -> Result<AccountOverview, sqlx::Error> {
    let rows = list(pool).await?;

    let accounts: Vec<AccountAssignmentRow> = rows
        .into_iter()
        .map(|account| AccountAssignmentRow {
            account_id: account.id,
            code: account.code,
            name: account.name,
            active: account.active,
            customer_id: None,
            customer_name: None,
            web_client_id: None,
            web_client_name: None,
            teamviewer_groups: Vec::new(),
        })
        .collect();

    let mut accounts = accounts;

    let customers: Vec<(String, String, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT c.id, c.name, c.account_id, a.code
         FROM customers c
         LEFT JOIN accounts a ON a.id = c.account_id
         WHERE c.active = true
         ORDER BY c.name",
    )
    .fetch_all(pool)
    .await?;

    let web_clients: Vec<(String, String, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT w.id, w.name, w.account_id, a.code
         FROM web_clients w
         LEFT JOIN accounts a ON a.id = w.account_id
         WHERE w.active = true
         ORDER BY w.name",
    )
    .fetch_all(pool)
    .await?;

    let groups: Vec<(String, String, Option<String>, Option<String>, Option<String>)> =
        sqlx::query_as(
            "SELECT g.group_id, g.name, c.account_id, w.account_id, a.code
             FROM teamviewer_groups g
             LEFT JOIN customers c ON c.id = g.customer_id
             LEFT JOIN web_clients w ON w.id = g.web_client_id
             LEFT JOIN accounts a ON a.id = COALESCE(c.account_id, w.account_id)
             WHERE g.ignored = false
             ORDER BY g.name",
        )
        .fetch_all(pool)
        .await?;

    // Je Konto den ersten zugeordneten Kunden und Web-Kunden eintragen.
    for account in accounts.iter_mut() {
        if let Some((id, name, _, _)) = customers
            .iter()
            .find(|c| c.2.as_deref() == Some(account.account_id.as_str()))
        {
            account.customer_id = Some(id.clone());
            account.customer_name = Some(name.clone());
        }

        if let Some((id, name, _, _)) = web_clients
            .iter()
            .find(|c| c.2.as_deref() == Some(account.account_id.as_str()))
        {
            account.web_client_id = Some(id.clone());
            account.web_client_name = Some(name.clone());
        }

        account.teamviewer_groups = groups
            .iter()
            .filter(|g| {
                g.2.as_deref() == Some(account.account_id.as_str())
                    || g.3.as_deref() == Some(account.account_id.as_str())
            })
            .map(|g| TeamViewerAssignment {
                group_id: g.0.clone(),
                name: g.1.clone(),
            })
            .collect();
    }

    let unassigned_customers = customers.iter().filter(|c| c.2.is_none()).count();
    let unassigned_teamviewer = groups.iter().filter(|g| g.4.is_none()).count();

    Ok(AccountOverview {
        accounts,
        customers: customers
            .into_iter()
            .map(|(id, name, _, code)| AssignmentTarget {
                id,
                name,
                taken_by: code,
            })
            .collect(),
        web_clients: web_clients
            .into_iter()
            .map(|(id, name, _, code)| AssignmentTarget {
                id,
                name,
                taken_by: code,
            })
            .collect(),
        teamviewer_groups: groups
            .into_iter()
            .map(|(id, name, _, _, code)| AssignmentTarget {
                id,
                name,
                taken_by: code,
            })
            .collect(),
        unassigned_customers,
        unassigned_teamviewer,
    })
}

/// Ordnet eine TeamViewer-Gruppe über das Konto zu.
///
/// Anders als in der TeamViewer-Ansicht wird hier **das Konto** gewählt: Die
/// Gruppe zeigt dann auf den ersten Kunden des Kontos. Das hält beide
/// Sichtweisen konsistent – wer hier zuordnet, sieht dasselbe in TeamViewer.
pub async fn assign_group_to_account(
    pool: &PgPool,
    group_id: &str,
    account_id: Option<&str>,
) -> Result<(), String> {
    let Some(account_id) = account_id else {
        return crate::teamviewer::assign(pool, group_id, None, None).await;
    };

    // Bevorzugt ein NinjaOne-Kunde des Kontos, sonst ein Web-Kunde.
    let customer: Option<String> =
        sqlx::query_scalar("SELECT id FROM customers WHERE account_id = $1 ORDER BY name LIMIT 1")
            .bind(account_id)
            .fetch_optional(pool)
            .await
            .map_err(|error| error.to_string())?;

    if customer.is_none() {
        let web: Option<String> = sqlx::query_scalar(
            "SELECT id FROM web_clients WHERE account_id = $1 ORDER BY name LIMIT 1",
        )
        .bind(account_id)
        .fetch_optional(pool)
        .await
        .map_err(|error| error.to_string())?;

        if web.is_none() {
            return Err(
                "Diesem Konto ist noch kein Kunde zugeordnet. Bitte zuerst in der Spalte 
                 NinjaOne oder Web einen Datensatz wählen."
                    .to_string(),
            );
        }

        return crate::teamviewer::assign(pool, group_id, None, web.as_deref()).await;
    }

    crate::teamviewer::assign(pool, group_id, customer.as_deref(), None).await
}

#[cfg(test)]
mod overview_tests {
    use super::*;
    use crate::testdb;

    #[tokio::test]
    async fn overview_shows_assigned_and_free_datasets() {
        let (_guard, pool) = testdb::fixture().await;
        let account = create(&pool, "MWK", "Musterwerk GmbH").await.unwrap().unwrap();

        sqlx::query(
            "INSERT INTO customers (id, name, short_code, active, created_at)
             VALUES ('c1', 'Musterwerk GmbH', 'MWK', true, $1),
                    ('c2', 'Ohne Konto', 'OHK', true, $1)",
        )
        .bind(now_iso())
        .execute(&pool)
        .await
        .unwrap();

        assign_customer(&pool, "c1", Some(&account.id)).await.unwrap();

        let view = overview(&pool).await.unwrap();

        let row = &view.accounts[0];
        assert_eq!(row.customer_name.as_deref(), Some("Musterwerk GmbH"));
        assert_eq!(view.unassigned_customers, 1);
        // Beide Kunden stehen im Menü, der vergebene mit Kontokürzel.
        assert_eq!(view.customers.len(), 2);
        assert_eq!(
            view.customers
                .iter()
                .find(|c| c.id == "c1")
                .and_then(|c| c.taken_by.clone())
                .as_deref(),
            Some("MWK")
        );
    }

    #[tokio::test]
    async fn a_group_is_assigned_to_the_accounts_customer() {
        let (_guard, pool) = testdb::fixture().await;
        let account = create(&pool, "MWK", "Musterwerk GmbH").await.unwrap().unwrap();

        sqlx::query(
            "INSERT INTO customers (id, name, short_code, active, created_at)
             VALUES ('c1', 'Musterwerk GmbH', 'MWK', true, $1)",
        )
        .bind(now_iso())
        .execute(&pool)
        .await
        .unwrap();
        assign_customer(&pool, "c1", Some(&account.id)).await.unwrap();

        sqlx::query(
            "INSERT INTO teamviewer_groups (group_id, name, updated_at)
             VALUES ('g1', 'Musterwerk', $1)",
        )
        .bind(now_iso())
        .execute(&pool)
        .await
        .unwrap();

        assign_group_to_account(&pool, "g1", Some(&account.id))
            .await
            .unwrap();

        let view = overview(&pool).await.unwrap();
        assert_eq!(view.accounts[0].teamviewer_groups.len(), 1);
        assert_eq!(view.unassigned_teamviewer, 0);
    }

    #[tokio::test]
    async fn an_empty_account_is_refused() {
        let (_guard, pool) = testdb::fixture().await;
        let account = create(&pool, "LEER", "Noch nichts").await.unwrap().unwrap();

        sqlx::query(
            "INSERT INTO teamviewer_groups (group_id, name, updated_at)
             VALUES ('g1', 'Irgendwas', $1)",
        )
        .bind(now_iso())
        .execute(&pool)
        .await
        .unwrap();

        let error = assign_group_to_account(&pool, "g1", Some(&account.id))
            .await
            .unwrap_err();
        assert!(error.contains("kein Kunde"), "{error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testdb;

    #[tokio::test]
    async fn code_is_normalized_so_case_does_not_create_duplicates() {
        let (_guard, pool) = testdb::fixture().await;

        assert!(create(&pool, "mwk", "Musterwerk GmbH").await.unwrap().is_some());
        assert!(
            create(&pool, " MWk ", "Nochmal").await.unwrap().is_none(),
            "dasselbe Kürzel in anderer Schreibweise darf kein zweites Konto anlegen"
        );
    }

    #[tokio::test]
    async fn customers_and_web_clients_can_share_one_account() {
        let (_guard, pool) = testdb::fixture().await;
        let account = create(&pool, "MWK", "Musterwerk GmbH").await.unwrap().unwrap();

        sqlx::query(
            "INSERT INTO customers (id, name, short_code, active, created_at)
             VALUES ('c1', 'Musterwerk GmbH', 'AS', true, $1)",
        )
        .bind(now_iso())
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO web_clients (id, name, short_code, active, created_at)
             VALUES ('w1', 'Musterwerk Web', 'MWKW', true, $1)",
        )
        .bind(now_iso())
        .execute(&pool)
        .await
        .unwrap();

        assign_customer(&pool, "c1", Some(&account.id)).await.unwrap();
        assign_web_client(&pool, "w1", Some(&account.id)).await.unwrap();

        let (customers, web_clients) = members(&pool, &account.id).await.unwrap();
        assert_eq!(customers, vec!["c1".to_string()]);
        assert_eq!(web_clients, vec!["w1".to_string()]);

        let listed = list(&pool).await.unwrap();
        assert_eq!(listed[0].customer_count, 1);
        assert_eq!(listed[0].web_client_count, 1);
    }

    #[tokio::test]
    async fn clearing_the_assignment_leaves_the_customer_intact() {
        let (_guard, pool) = testdb::fixture().await;
        let account = create(&pool, "MWK", "Musterwerk GmbH").await.unwrap().unwrap();

        sqlx::query(
            "INSERT INTO customers (id, name, short_code, active, created_at)
             VALUES ('c1', 'Musterwerk GmbH', 'AS', true, $1)",
        )
        .bind(now_iso())
        .execute(&pool)
        .await
        .unwrap();

        assign_customer(&pool, "c1", Some(&account.id)).await.unwrap();
        assign_customer(&pool, "c1", None).await.unwrap();

        let account_id: Option<String> =
            sqlx::query_scalar("SELECT account_id FROM customers WHERE id = 'c1'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(account_id.is_none());
    }
}
