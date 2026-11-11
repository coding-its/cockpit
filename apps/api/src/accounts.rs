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
