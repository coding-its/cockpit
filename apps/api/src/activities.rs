//! Leistungsarten.
//!
//! Die Kategorien, unter denen Zeiten erfasst werden. Sie sind pflegbar, weil
//! jede Firma anders einteilt – und weil eine fest eingebaute Liste nach dem
//! ersten Monat nicht mehr passt.
//!
//! `time_entries.activity` bleibt freier Text: Ein Eintrag behält die
//! Bezeichnung, die beim Erfassen galt. Wird eine Kategorie später umbenannt,
//! ändern sich alte Nachweise dadurch nicht.

use serde::Serialize;
use sqlx::PgPool;

use crate::util::now_iso;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ActivityKind {
    pub id: String,
    pub name: String,
    pub position: i64,
    pub active: bool,
}

/// Vorgabe, wenn eine Verbindung keine Leistungsart bekommen hat.
pub const DEFAULT_ACTIVITY: &str = "Remote Support";

/// Alle Leistungsarten in Anzeigereihenfolge.
pub async fn list(pool: &PgPool, include_inactive: bool) -> Result<Vec<ActivityKind>, sqlx::Error> {
    sqlx::query_as::<_, ActivityKind>(
        "SELECT id, name, position, active
         FROM activity_kinds
         WHERE ($1 OR active = true)
         ORDER BY position, name",
    )
    .bind(include_inactive)
    .fetch_all(pool)
    .await
}

/// Legt eine Leistungsart an. `None`, wenn der Name schon vergeben ist.
pub async fn create(pool: &PgPool, name: &str) -> Result<Option<ActivityKind>, sqlx::Error> {
    let name = name.trim();
    let id = uuid::Uuid::new_v4().to_string();

    // Neue Einträge hinten anstellen.
    let next: i64 = sqlx::query_scalar("SELECT COALESCE(MAX(position), 0) + 10 FROM activity_kinds")
        .fetch_one(pool)
        .await?;

    let inserted = sqlx::query(
        "INSERT INTO activity_kinds (id, name, position, active, created_at)
         VALUES ($1, $2, $3, true, $4)
         ON CONFLICT DO NOTHING",
    )
    .bind(&id)
    .bind(name)
    .bind(next)
    .bind(now_iso())
    .execute(pool)
    .await?;

    if inserted.rows_affected() == 0 {
        return Ok(None);
    }

    sqlx::query_as::<_, ActivityKind>(
        "SELECT id, name, position, active FROM activity_kinds WHERE id = $1",
    )
    .bind(&id)
    .fetch_optional(pool)
    .await
}

/// Ändert Name oder Sichtbarkeit. `None`, wenn der Name vergeben ist.
pub async fn update(
    pool: &PgPool,
    id: &str,
    name: Option<&str>,
    active: Option<bool>,
) -> Result<Option<ActivityKind>, sqlx::Error> {
    if let Some(name) = name {
        let name = name.trim();
        if name.is_empty() {
            return Ok(None);
        }

        let taken: Option<String> = sqlx::query_scalar(
            "SELECT id FROM activity_kinds WHERE lower(name) = lower($1) AND id <> $2",
        )
        .bind(name)
        .bind(id)
        .fetch_optional(pool)
        .await?;

        if taken.is_some() {
            return Ok(None);
        }

        sqlx::query("UPDATE activity_kinds SET name = $1 WHERE id = $2")
            .bind(name)
            .bind(id)
            .execute(pool)
            .await?;
    }

    if let Some(active) = active {
        sqlx::query("UPDATE activity_kinds SET active = $1 WHERE id = $2")
            .bind(active)
            .bind(id)
            .execute(pool)
            .await?;
    }

    sqlx::query_as::<_, ActivityKind>(
        "SELECT id, name, position, active FROM activity_kinds WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
}

/// Prüft, ob ein Name einer aktiven Leistungsart entspricht.
///
/// Der Import schreibt den Namen als Text; so landet nichts, was es nicht
/// (mehr) gibt. Groß- und Kleinschreibung spielt keine Rolle.
pub async fn canonical(pool: &PgPool, name: &str) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar::<_, String>(
        "SELECT name FROM activity_kinds WHERE lower(name) = lower($1) AND active = true",
    )
    .bind(name.trim())
    .fetch_optional(pool)
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testdb;

    #[tokio::test]
    async fn the_shipped_list_is_there() {
        let (_guard, pool) = testdb::fixture().await;

        let kinds = list(&pool, false).await.unwrap();
        assert!(kinds.len() >= 14, "{kinds:?}");
        assert!(kinds.iter().any(|k| k.name == DEFAULT_ACTIVITY));
    }

    #[tokio::test]
    async fn names_are_unique_regardless_of_case() {
        let (_guard, pool) = testdb::fixture().await;

        assert!(create(&pool, "Schulung").await.unwrap().is_some());
        assert!(
            create(&pool, " schulung ").await.unwrap().is_none(),
            "dieselbe Leistungsart in anderer Schreibweise darf nicht doppelt entstehen"
        );
    }

    #[tokio::test]
    async fn inactive_kinds_disappear_from_the_picker() {
        let (_guard, pool) = testdb::fixture().await;
        let kind = create(&pool, "Schulung").await.unwrap().unwrap();

        update(&pool, &kind.id, None, Some(false)).await.unwrap();

        let active = list(&pool, false).await.unwrap();
        assert!(!active.iter().any(|k| k.id == kind.id));

        let all = list(&pool, true).await.unwrap();
        assert!(all.iter().any(|k| k.id == kind.id));
    }

    #[tokio::test]
    async fn canonical_finds_the_stored_spelling() {
        let (_guard, pool) = testdb::fixture().await;
        create(&pool, "Schulung").await.unwrap();

        assert_eq!(
            canonical(&pool, "schulung").await.unwrap().as_deref(),
            Some("Schulung")
        );
        assert!(canonical(&pool, "Unbekannt").await.unwrap().is_none());
    }
}
