//! Einstellungen, die im Cockpit gepflegt werden.
//!
//! Bewusst wenige: Stundensätze und Steuersatz. Alles andere gehört in die
//! `.env` (Zugangsdaten) oder in die Stammdaten (Verträge je Kunde).
//!
//! Ablage in `app_settings` als Text. Fehlt ein Eintrag, gilt der Standardwert –
//! so funktioniert eine frische Datenbank ohne Einrichtung.

use serde::{Deserialize, Serialize};
use sqlx::PgPool;

/// Schlüssel in `app_settings`.
const KEY_DEFAULT_RATE: &str = "billing.default_hourly_rate";
const KEY_VAT_RATE: &str = "billing.vat_rate";

/// Standard-Stundensatz netto, wenn nichts gepflegt ist (150,00 € brutto).
pub const FALLBACK_HOURLY_RATE: f64 = 126.05;
/// Umsatzsteuersatz in Prozent.
pub const FALLBACK_VAT_RATE: f64 = 19.0;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BillingSettings {
    /// Stundensatz netto, der greift, wenn im Vertrag keiner steht.
    pub default_hourly_rate: f64,
    /// Umsatzsteuersatz in Prozent.
    pub vat_rate: f64,
}

#[derive(Debug, Deserialize)]
pub struct UpdateSettingsRequest {
    pub default_hourly_rate: Option<f64>,
    pub vat_rate: Option<f64>,

}

impl Default for BillingSettings {
    fn default() -> Self {
        Self {
            default_hourly_rate: FALLBACK_HOURLY_RATE,
            vat_rate: FALLBACK_VAT_RATE,
        }
    }
}

/// Liest einen Zahlenwert, ohne bei fehlendem oder kaputtem Eintrag zu scheitern.
async fn read_number(pool: &PgPool, key: &str) -> Option<f64> {
    let raw: Option<String> = sqlx::query_scalar("SELECT value FROM app_settings WHERE key = $1")
        .bind(key)
        .fetch_optional(pool)
        .await
        .ok()?;

    raw?.trim().replace(',', ".").parse::<f64>().ok()
}

pub async fn billing(pool: &PgPool) -> BillingSettings {
    let defaults = BillingSettings::default();

    let default_hourly_rate = read_number(pool, KEY_DEFAULT_RATE)
        .await
        .filter(|value| *value > 0.0)
        .unwrap_or(defaults.default_hourly_rate);

    let vat_rate = read_number(pool, KEY_VAT_RATE)
        .await
        .filter(|value| *value >= 0.0 && *value <= 30.0)
        .unwrap_or(defaults.vat_rate);

    BillingSettings {
        default_hourly_rate,
        vat_rate,
    }
}

/// Nur der Stundensatz – für Abrechnung und Bericht.
pub async fn default_hourly_rate(pool: &PgPool) -> f64 {
    billing(pool).await.default_hourly_rate
}

async fn write(pool: &PgPool, key: &str, value: &str) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO app_settings (key, value) VALUES ($1, $2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
    )
    .bind(key)
    .bind(value)
    .execute(pool)
    .await?;
    Ok(())
}

/// Speichert die übergebenen Werte und gibt den vollständigen Stand zurück.
pub async fn update(
    pool: &PgPool,
    payload: &UpdateSettingsRequest,
) -> Result<BillingSettings, String> {
    if let Some(rate) = payload.default_hourly_rate {
        if !(0.0..=1000.0).contains(&rate) || rate <= 0.0 {
            return Err("Der Stundensatz muss zwischen 0 und 1000 liegen.".to_string());
        }
        write(pool, KEY_DEFAULT_RATE, &rate.to_string())
            .await
            .map_err(|error| error.to_string())?;
    }

    if let Some(vat) = payload.vat_rate {
        if !(0.0..=30.0).contains(&vat) {
            return Err("Der Steuersatz muss zwischen 0 und 30 Prozent liegen.".to_string());
        }
        write(pool, KEY_VAT_RATE, &vat.to_string())
            .await
            .map_err(|error| error.to_string())?;
    }

    Ok(billing(pool).await)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn falls_back_to_defaults_on_empty_settings() {
        let (_guard, pool) = crate::testdb::fixture().await;
            sqlx::query(
                "INSERT INTO customers (id, name, short_code, created_at) VALUES ('c1', 'Testkunde', 'TC', '2026-01-01')",
            )
            .execute(&pool)
            .await
            .unwrap();
        

        let settings = billing(&pool).await;
        assert_eq!(settings.default_hourly_rate, FALLBACK_HOURLY_RATE);
        assert_eq!(settings.vat_rate, FALLBACK_VAT_RATE);

    }

    #[tokio::test]
    async fn stores_and_reads_values() {
        let (_guard, pool) = crate::testdb::fixture().await;
            sqlx::query(
                "INSERT INTO customers (id, name, short_code, created_at) VALUES ('c1', 'Testkunde', 'TC', '2026-01-01')",
            )
            .execute(&pool)
            .await
            .unwrap();
        

        let updated = update(
            &pool,
            &UpdateSettingsRequest {
                default_hourly_rate: Some(98.5),
                vat_rate: Some(7.0),

            },
        )
        .await
        .unwrap();

        assert_eq!(updated.default_hourly_rate, 98.5);
        assert_eq!(updated.vat_rate, 7.0);
        assert_eq!(billing(&pool).await.default_hourly_rate, 98.5);
    }

    #[tokio::test]
    async fn rejects_absurd_rates() {
        let (_guard, pool) = crate::testdb::fixture().await;
            sqlx::query(
                "INSERT INTO customers (id, name, short_code, created_at) VALUES ('c1', 'Testkunde', 'TC', '2026-01-01')",
            )
            .execute(&pool)
            .await
            .unwrap();
        

        let error = update(
            &pool,
            &UpdateSettingsRequest {
                default_hourly_rate: Some(-5.0),
                vat_rate: None,

            },
        )
        .await
        .unwrap_err();

        assert!(error.contains("Stundensatz"));
    }
}
