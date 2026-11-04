use sqlx::PgPool;

use crate::models::{ActiveTimer, TimeEntryListItem};

/// Gemeinsame Auswahlliste fuer Zeiteintraege beider Zweige.
/// Ein Eintrag gehoert entweder zu einem NinjaOne-Kunden oder zu einem Web-Kunden.
const SELECT_ENTRY: &str = r#"
    SELECT
        te.id,
        te.customer_id,
        te.web_client_id,
        COALESCE(c.name, w.name, 'Ohne Zuordnung') AS owner_name,
        COALESCE(c.short_code, w.short_code, '--') AS owner_short_code,
        s.label AS site_label,
        d.hostname AS device_label,
        COALESCE(c.name, w.name, 'Ohne Zuordnung') AS customer_name,
        COALESCE(c.short_code, w.short_code, '--') AS customer_short_code,
        te.run_id,
        te.task_id,
        t.title AS task_title,
        te.user_name,
        te.activity,
        te.note,
        te.started_at,
        te.ended_at,
        te.duration_seconds,
        te.billable,
        te.source,
        te.billed_at,
        COALESCE(
            (SELECT hourly_rate FROM contracts WHERE customer_id = te.customer_id AND active = true LIMIT 1),
            (SELECT hourly_rate FROM web_clients WHERE id = te.web_client_id),
            0.0
        ) AS hourly_rate
    FROM time_entries te
    LEFT JOIN customers c ON c.id = te.customer_id
    LEFT JOIN web_clients w ON w.id = te.web_client_id
    LEFT JOIN sites s ON s.id = te.site_id
    LEFT JOIN devices d ON d.id = te.device_id
    LEFT JOIN tasks t ON t.id = te.task_id
"#;

/// Der laufende Timer **dieser Person**. Mehrere Personen dürfen gleichzeitig
/// messen; jeder sieht in der Kopfleiste nur seinen eigenen.
pub async fn active_timer(
    pool: &PgPool,
    user_id: &str,
) -> Result<Option<ActiveTimer>, sqlx::Error> {
    sqlx::query_as::<_, ActiveTimer>(
        r#"
        SELECT
            te.id,
            te.user_name,
            te.web_client_id,
            te.device_id,
            dev.hostname AS device_hostname,
            COALESCE(c.name, w.name, 'Ohne Zuordnung') AS customer_name,
            COALESCE(c.short_code, w.short_code, '--') AS customer_short_code,
            te.task_id,
            t.title AS task_title,
            te.activity,
            te.note,
            te.started_at,
            te.billable
        FROM time_entries te
        LEFT JOIN customers c ON c.id = te.customer_id
        LEFT JOIN web_clients w ON w.id = te.web_client_id
        LEFT JOIN devices dev ON dev.id = te.device_id
        LEFT JOIN tasks t ON t.id = te.task_id
        WHERE te.ended_at IS NULL AND te.user_id = $1
        ORDER BY te.started_at DESC
        LIMIT 1
        "#,
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
}

/// Alle gerade laufenden Timer – für die Team-Übersicht („wer arbeitet woran").
pub async fn running_timers(
    pool: &PgPool,
) -> Result<Vec<ActiveTimer>, sqlx::Error> {
    sqlx::query_as::<_, ActiveTimer>(
        r#"
        SELECT
            te.id,
            te.user_name,
            te.web_client_id,
            te.device_id,
            dev.hostname AS device_hostname,
            COALESCE(c.name, w.name, 'Ohne Zuordnung') AS customer_name,
            COALESCE(c.short_code, w.short_code, '--') AS customer_short_code,
            te.task_id,
            t.title AS task_title,
            te.activity,
            te.note,
            te.started_at,
            te.billable
        FROM time_entries te
        LEFT JOIN customers c ON c.id = te.customer_id
        LEFT JOIN web_clients w ON w.id = te.web_client_id
        LEFT JOIN devices dev ON dev.id = te.device_id
        LEFT JOIN tasks t ON t.id = te.task_id
        WHERE te.ended_at IS NULL
        ORDER BY te.started_at DESC
        "#,
    )
    .fetch_all(pool)
    .await
}

pub async fn time_entries(
    pool: &PgPool,
    customer_id: Option<&str>,
    web_client_id: Option<&str>,
    month: Option<&str>,
    limit: i64,
) -> Result<Vec<TimeEntryListItem>, sqlx::Error> {
    let sql = format!(
        "{SELECT_ENTRY}
        WHERE ($1 IS NULL OR te.customer_id = $1)
          AND ($2 IS NULL OR te.web_client_id = $2)
          AND ($3 IS NULL OR substr(te.started_at, 1, 7) = $3)
        ORDER BY te.started_at DESC
        LIMIT $4"
    );

    sqlx::query_as::<_, TimeEntryListItem>(&sql)
        .bind(customer_id)
        .bind(web_client_id)
        .bind(month)
        .bind(limit)
        .fetch_all(pool)
        .await
}

pub async fn time_entry(
    pool: &PgPool,
    id: &str,
) -> Result<Option<TimeEntryListItem>, sqlx::Error> {
    let sql = format!("{SELECT_ENTRY} WHERE te.id = $1");

    sqlx::query_as::<_, TimeEntryListItem>(&sql)
        .bind(id)
        .fetch_optional(pool)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn insert_user(pool: &PgPool, id: &str, name: &str) {
        sqlx::query(
            "INSERT INTO app_users (id, username, display_name, password_hash, role, created_at)
             VALUES ($1, $1, $2, 'x', 'technician', '2026-01-01')",
        )
        .bind(id)
        .bind(name)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn insert_entry(
        pool: &PgPool,
        id: &str,
        user_id: &str,
        started: &str,
        ended: Option<&str>,
    ) {
        sqlx::query(
            "INSERT INTO time_entries
                 (id, user_id, user_name, activity, started_at, ended_at, billable, created_at)
             VALUES ($1, $2, $2, 'Test', $3, $4, true, $3)",
        )
        .bind(id)
        .bind(user_id)
        .bind(started)
        .bind(ended)
        .execute(pool)
        .await
        .unwrap();
    }

    /// Der eigentliche Fehler von vorher: Ein Start hat den neuesten offenen
    /// Eintrag im ganzen System beendet – auch den einer anderen Person.
    #[tokio::test]
    async fn active_timer_belongs_to_one_person_only() {
        let (_guard, pool) = crate::testdb::fixture().await;
        insert_user(&pool, "anna", "Anna").await;
        insert_user(&pool, "ben", "Ben").await;
        insert_entry(&pool, "a", "anna", "2026-10-26T08:00:00Z", None).await;
        insert_entry(&pool, "b", "ben", "2026-10-26T09:00:00Z", None).await;

        let anna = active_timer(&pool, "anna").await.unwrap().unwrap();
        let ben = active_timer(&pool, "ben").await.unwrap().unwrap();

        assert_eq!(anna.id, "a");
        assert_eq!(ben.id, "b");
    }

    #[tokio::test]
    async fn finished_entries_are_not_active() {
        let (_guard, pool) = crate::testdb::fixture().await;
        insert_user(&pool, "anna", "Anna").await;
        insert_entry(
            &pool,
            "a",
            "anna",
            "2026-10-26T08:00:00Z",
            Some("2026-10-26T09:00:00Z"),
        )
        .await;

        assert!(active_timer(&pool, "anna").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn running_timers_lists_everyone() {
        let (_guard, pool) = crate::testdb::fixture().await;
        insert_user(&pool, "anna", "Anna").await;
        insert_user(&pool, "ben", "Ben").await;
        insert_entry(&pool, "a", "anna", "2026-10-26T08:00:00Z", None).await;
        insert_entry(&pool, "b", "ben", "2026-10-26T09:00:00Z", None).await;
        insert_entry(
            &pool,
            "c",
            "ben",
            "2026-10-26T07:00:00Z",
            Some("2026-10-26T07:30:00Z"),
        )
        .await;

        let running = running_timers(&pool).await.unwrap();
        assert_eq!(running.len(), 2);
    }
}
