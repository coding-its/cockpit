//! Patch- und Antivirenstand aus NinjaOne.
//!
//! NinjaOne liefert beide Angaben für den ganzen Mandanten in je einem Aufruf
//! (`/v2/queries/os-patches` und `/v2/queries/antivirus-status`). Hier werden
//! sie auf die lokalen Geräte verteilt und zu einer Ampel verdichtet, die die
//! Oberfläche versteht:
//!
//! - `patch_status`: `pending` (genehmigte Patches offen), `ok` (gescannt,
//!   nichts offen), `unknown` (keine Daten – z. B. noch nie gescannt)
//! - `antivirus_status`: `ok`, `warning`, `unknown`
//!
//! Wichtig bei Antivirus: Ein Gerät meldet mehrere Produkte. Läuft Bitdefender,
//! steht Defender auf `OFF` – das ist gewollt und darf keine Warnung erzeugen.
//! Deshalb gewinnt ein Drittanbieter-Produkt immer gegen Defender.

use std::collections::HashMap;

use anyhow::Result;
use sqlx::PgPool;

use crate::ninjaone::{AntivirusEntry, NinjaOneClient, OsPatch, SoftwarePatch};
use crate::util::now_iso;

/// Statuswerte, die einen Patch als "noch nicht installiert, aber gewollt" führen.
/// `REJECTED` fehlt hier absichtlich: abgelehnt heißt entschieden.
const PENDING_STATUSES: [&str; 3] = ["APPROVED", "DELAYED", "MANUAL"];

#[derive(Debug, Default, Clone, serde::Serialize)]
pub struct MonitoringSummary {
    pub devices_total: usize,
    pub devices_with_patch_data: usize,
    pub devices_with_pending_patches: usize,
    pub devices_with_av_data: usize,
    pub devices_with_av_warning: usize,
    /// Offene Betriebssystem-Patches.
    pub pending_os: usize,
    /// Offene Software-Patches.
    pub pending_software: usize,
    /// Abgelehnte Betriebssystem-Patches.
    pub rejected_os: usize,
    /// Abgelehnte Software-Patches.
    pub rejected_software: usize,
    pub message: String,
}

impl MonitoringSummary {
    pub fn rejected_patches(&self) -> usize {
        self.rejected_os + self.rejected_software
    }
}

/// Verdichteter Zustand eines Geräts, wie er in `devices` landet.
#[derive(Debug, Default)]
struct DeviceState {
    patch_pending: i64,
    patch_rejected: i64,
    has_patch_rows: bool,
    antivirus_product: Option<String>,
    antivirus_state: Option<String>,
    antivirus_definitions: Option<String>,
}

/// Holt Patch- und Antivirusstand und schreibt sie auf die Geräte.
pub async fn refresh(pool: &PgPool, client: &NinjaOneClient) -> Result<MonitoringSummary> {
    let os_patches = client.os_patches().await?;
    let software_patches = client.software_patches().await?;
    let antivirus = client.antivirus_status().await?;
    persist(pool, &os_patches, &software_patches, &antivirus).await
}

/// Schreibt die bereits geholten Daten weg – getrennt von `refresh`, damit sich
/// die Verdichtung ohne Netzwerk testen lässt.
pub async fn persist(
    pool: &PgPool,
    patches: &[OsPatch],
    software: &[SoftwarePatch],
    antivirus: &[AntivirusEntry],
) -> Result<MonitoringSummary> {
    let now = now_iso();
    let mut states: HashMap<i64, DeviceState> = HashMap::new();

    for patch in patches {
        let state = states.entry(patch.device_id).or_default();
        state.has_patch_rows = true;
        match normalize_status(patch.status.as_deref()) {
            "REJECTED" => state.patch_rejected += 1,
            other if PENDING_STATUSES.contains(&other) => state.patch_pending += 1,
            _ => {}
        }
    }

    // Software-Patches zählen in dieselbe Ampel: offen ist offen, egal ob
    // Betriebssystem oder Programm. Die Aufteilung steht in der Detailansicht.
    for patch in software {
        let state = states.entry(patch.device_id).or_default();
        state.has_patch_rows = true;
        match normalize_status(patch.status.as_deref()) {
            "REJECTED" => state.patch_rejected += 1,
            other if PENDING_STATUSES.contains(&other) => state.patch_pending += 1,
            _ => {}
        }
    }

    for entry in antivirus {
        let state = states.entry(entry.device_id).or_default();
        let keep_current = match &state.antivirus_product {
            None => true,
            Some(current) => {
                rank(entry.product_name.as_deref(), entry.product_state.as_deref())
                    > rank(Some(current), state.antivirus_state.as_deref())
            }
        };

        if keep_current {
            // NinjaOne meldet für Geräte ohne erkannten Schutz den Produktnamen
            // "NONE" – das ist kein Produkt und gehört nicht in die Anzeige.
            state.antivirus_product = clean_product(entry.product_name.as_deref());
            state.antivirus_state = entry.product_state.clone();
            state.antivirus_definitions = entry.definition_status.clone();
        }
    }

    let mut summary = MonitoringSummary::default();

    for (ninja_id, state) in &states {
        let patch_status = if state.patch_pending > 0 {
            "pending"
        } else if state.has_patch_rows {
            "ok"
        } else {
            "unknown"
        };

        let antivirus_status = match state.antivirus_state.as_deref() {
            None => "unknown",
            Some("ON") if !definitions_stale(state.antivirus_definitions.as_deref()) => "ok",
            Some(_) => "warning",
        };

        let result = sqlx::query(
            "UPDATE devices SET
                 patch_status = $2,
                 patch_pending = $3,
                 patch_rejected = $4,
                 patch_checked_at = $5,
                 antivirus_status = $6,
                 antivirus_product = $7,
                 antivirus_state = $8,
                 antivirus_definitions = $9,
                 antivirus_checked_at = $10
             WHERE ninja_device_id = $1",
        )
        .bind(ninja_id)
        .bind(patch_status)
        .bind(state.patch_pending)
        .bind(state.patch_rejected)
        .bind(&now)
        .bind(antivirus_status)
        .bind(state.antivirus_product.as_deref())
        .bind(state.antivirus_state.as_deref())
        .bind(state.antivirus_definitions.as_deref())
        .bind(&now)
        .execute(pool)
        .await?;

        if result.rows_affected() == 0 {
            continue;
        }

        summary.devices_total += 1;
        if state.has_patch_rows {
            summary.devices_with_patch_data += 1;
        }
        if state.patch_pending > 0 {
            summary.devices_with_pending_patches += 1;
        }
        if state.antivirus_product.is_some() {
            summary.devices_with_av_data += 1;
        }
        if antivirus_status == "warning" {
            summary.devices_with_av_warning += 1;
        }
    }

    for patch in patches {
        match normalize_status(patch.status.as_deref()) {
            "REJECTED" => summary.rejected_os += 1,
            other if PENDING_STATUSES.contains(&other) => summary.pending_os += 1,
            _ => {}
        }
    }

    for patch in software {
        match normalize_status(patch.status.as_deref()) {
            "REJECTED" => summary.rejected_software += 1,
            other if PENDING_STATUSES.contains(&other) => summary.pending_software += 1,
            _ => {}
        }
    }

    replace_patches(pool, patches, software, &now).await?;

    summary.message = format!(
        "{} Geräte geprüft: {} mit Patchdaten ({} OS- und {} Software-Patches offen, {} abgelehnt), {} mit Antivirusdaten ({} zu prüfen).",
        summary.devices_total,
        summary.devices_with_patch_data,
        summary.pending_os,
        summary.pending_software,
        summary.rejected_patches(),
        summary.devices_with_av_data,
        summary.devices_with_av_warning
    );

    Ok(summary)
}

/// Ersetzt den gespeicherten Patchstand vollständig.
///
/// Die Abfragen liefern den aktuellen Stand des ganzen Mandanten – alte Zeilen
/// wären also veraltet. Geschrieben wird in einer Transaktion: bei rund 24.000
/// Software-Patch-Zeilen dauert das einzeln sonst Minuten.
async fn replace_patches(
    pool: &PgPool,
    patches: &[OsPatch],
    software: &[SoftwarePatch],
    now: &str,
) -> Result<()> {
    // Einmal fragen, welche Geräte bekannt sind – nicht 25.000-mal.
    let known: Vec<String> = sqlx::query_scalar("SELECT id FROM devices")
        .fetch_all(pool)
        .await?;
    let known: std::collections::HashSet<String> = known.into_iter().collect();

    let mut tx = pool.begin().await?;

    sqlx::query("DELETE FROM device_patches")
        .execute(&mut *tx)
        .await?;

    for patch in patches {
        let device_id = format!("ninja-dev-{}", patch.device_id);
        if !known.contains(&device_id) {
            continue;
        }

        sqlx::query(
            "INSERT INTO device_patches
             (device_id, patch_id, name, kb_number, severity, status, patch_type, observed_at, patch_kind)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'os')
             ON CONFLICT (device_id, patch_id) DO UPDATE SET
                 name = EXCLUDED.name, kb_number = EXCLUDED.kb_number,
                 severity = EXCLUDED.severity, status = EXCLUDED.status,
                 patch_type = EXCLUDED.patch_type, observed_at = EXCLUDED.observed_at,
                 patch_kind = EXCLUDED.patch_kind",
        )
        .bind(&device_id)
        .bind(&patch.id)
        .bind(patch.name.as_deref().unwrap_or("Unbenannter Patch"))
        .bind(patch.kb_number.as_deref())
        .bind(normalize_severity(patch.severity.as_deref()))
        .bind(normalize_status(patch.status.as_deref()))
        .bind(patch.patch_type.as_deref())
        .bind(now)
        .execute(&mut *tx)
        .await?;
    }

    for patch in software {
        let device_id = format!("ninja-dev-{}", patch.device_id);
        if !known.contains(&device_id) {
            continue;
        }

        sqlx::query(
            "INSERT INTO device_patches
             (device_id, patch_id, name, kb_number, severity, status, patch_type, observed_at, patch_kind)
             VALUES ($1, $2, $3, NULL, $4, $5, $6, $7, 'software')
             ON CONFLICT (device_id, patch_id) DO UPDATE SET
                 name = EXCLUDED.name, severity = EXCLUDED.severity,
                 status = EXCLUDED.status, patch_type = EXCLUDED.patch_type,
                 observed_at = EXCLUDED.observed_at, patch_kind = EXCLUDED.patch_kind",
        )
        .bind(&device_id)
        .bind(&patch.id)
        .bind(patch.title.as_deref().unwrap_or("Unbenanntes Programm"))
        .bind(normalize_severity(patch.impact.as_deref()))
        .bind(normalize_status(patch.status.as_deref()))
        .bind(patch.patch_type.as_deref())
        .bind(now)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;

    Ok(())
}

/// Status in Großbuchstaben – NinjaOne liefert gelegentlich gemischte Schreibweisen.
fn normalize_status(value: Option<&str>) -> &str {
    match value.unwrap_or("").to_ascii_uppercase().as_str() {
        "APPROVED" => "APPROVED",
        "REJECTED" => "REJECTED",
        "DELAYED" => "DELAYED",
        "MANUAL" => "MANUAL",
        "INSTALLED" => "INSTALLED",
        _ => "UNKNOWN",
    }
}

/// Auch die Schweregrade kommen gemischt (`OPTIONAL` und `optional`).
fn normalize_severity(value: Option<&str>) -> Option<String> {
    let upper = value?.to_ascii_uppercase();
    match upper.as_str() {
        "" => None,
        "OPTIONAL" => Some("OPTIONAL".to_string()),
        "IMPORTANT" | "CRITICAL" | "MODERATE" | "LOW" | "NONE" | "UNASSIGNED" => Some(upper),
        other => Some(other.to_string()),
    }
}

/// Erkennt Microsofts eingebauten Schutz.
///
/// Achtung: „Bitdefender" enthält ebenfalls „defender" – deshalb wird auf die
/// Microsoft-Namen geprüft und Bitdefender vorher ausgeschlossen.
/// Leere und Platzhalter-Produktnamen zu `None`.
fn clean_product(value: Option<&str>) -> Option<String> {
    let name = value?.trim();
    if name.is_empty() || name.eq_ignore_ascii_case("none") || name.eq_ignore_ascii_case("unknown") {
        return None;
    }
    Some(name.to_string())
}

fn is_defender(product: Option<&str>) -> bool {
    let Some(name) = product else {
        return false;
    };
    let lower = name.to_ascii_lowercase();

    !lower.contains("bitdefender")
        && (lower.contains("windows defender")
            || lower.contains("microsoft defender")
            || lower.contains("microsoft security"))
}

fn state_rank(state: Option<&str>) -> i32 {
    match state {
        Some("ON") => 3,
        Some("Expired") => 2,
        Some("OFF") => 1,
        _ => 0,
    }
}

/// Höher gewinnt.
///
/// Zuerst zählt der Zustand: ein laufendes Produkt schützt das Gerät, ein
/// abgeschaltetes nicht. Erst bei Gleichstand gewinnt das Drittanbieter-Produkt
/// gegen Defender – so steht in der Anzeige der Schutz, der wirklich arbeitet.
fn rank(product: Option<&str>, state: Option<&str>) -> (i32, bool) {
    (state_rank(state), !is_defender(product))
}

fn definitions_stale(status: Option<&str>) -> bool {
    status
        .map(|value| value.eq_ignore_ascii_case("Out-of-Date"))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn patch(device_id: i64, status: &str) -> OsPatch {
        OsPatch {
            id: format!("p-{device_id}-{status}"),
            name: Some("Test".to_string()),
            severity: Some("IMPORTANT".to_string()),
            status: Some(status.to_string()),
            patch_type: None,
            kb_number: Some("KB1".to_string()),
            device_id,
            timestamp: None,
        }
    }

    fn av(device_id: i64, product: &str, state: &str, definitions: &str) -> AntivirusEntry {
        AntivirusEntry {
            product_name: Some(product.to_string()),
            product_state: Some(state.to_string()),
            definition_status: Some(definitions.to_string()),
            version: None,
            device_id,
            timestamp: None,
        }
    }

    #[test]
    fn patch_status_counts_only_wanted_patches() {
        // Abgelehnte Patches sind eine Entscheidung, keine offene Arbeit.
        let rejected_only = [patch(1, "REJECTED")];
        let pending = [patch(2, "APPROVED"), patch(2, "DELAYED")];
        let all = [rejected_only[0].clone(), pending[0].clone(), pending[1].clone()];

        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        runtime.block_on(async {
            let (_guard, pool) = crate::testdb::fixture().await;
            sqlx::query(
                "INSERT INTO customers (id, name, short_code, created_at) VALUES ('c1', 'Testkunde', 'TC', '2026-01-01')",
            )
            .execute(&pool)
            .await
            .unwrap();
            
            

            for id in [1, 2] {
                sqlx::query("INSERT INTO devices (id, customer_id, ninja_device_id, hostname) VALUES ($1, 'c1', $2, $1)")
                    .bind(format!("ninja-dev-{id}"))
                    .bind(id)
                    .execute(&pool)
                    .await
                    .unwrap();
            }

            persist(&pool, &all, &[], &[]).await.unwrap();

            let one: (String, i64, i64) = sqlx::query_as(
                "SELECT patch_status, patch_pending, patch_rejected FROM devices WHERE ninja_device_id = 1",
            )
            .fetch_one(&pool)
            .await
            .unwrap();
            assert_eq!(one.0, "ok", "nur abgelehnte Patches gelten als erledigt");
            assert_eq!(one.1, 0);
            assert_eq!(one.2, 1);

            let two: (String, i64) = sqlx::query_as(
                "SELECT patch_status, patch_pending FROM devices WHERE ninja_device_id = 2",
            )
            .fetch_one(&pool)
            .await
            .unwrap();
            assert_eq!(two.0, "pending");
            assert_eq!(two.1, 2);
        });
    }

    #[test]
    fn third_party_antivirus_beats_disabled_defender() {
        let entries = [
            av(1, "Windows Defender", "OFF", "Up-to-Date"),
            av(1, "Bitdefender Endpoint Security Tools Antimalware", "ON", "Up-to-Date"),
        ];

        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        runtime.block_on(async {
            let (_guard, pool) = crate::testdb::fixture().await;
            sqlx::query(
                "INSERT INTO customers (id, name, short_code, created_at) VALUES ('c1', 'Testkunde', 'TC', '2026-01-01')",
            )
            .execute(&pool)
            .await
            .unwrap();
            
            
            sqlx::query("INSERT INTO devices (id, customer_id, ninja_device_id, hostname) VALUES ('ninja-dev-1', 'c1', 1, 'TEST-1')")
                .execute(&pool)
                .await
                .unwrap();

            persist(&pool, &[], &[], &entries).await.unwrap();

            let row: (String, String) = sqlx::query_as(
                "SELECT antivirus_status, antivirus_product FROM devices WHERE ninja_device_id = 1",
            )
            .fetch_one(&pool)
            .await
            .unwrap();

            assert_eq!(row.0, "ok", "abgeschalteter Defender allein ist keine Warnung");
            assert!(row.1.contains("Bitdefender"));
        });
    }

    #[test]
    fn placeholder_product_names_are_dropped() {
        assert_eq!(clean_product(Some("NONE")), None);
        assert_eq!(clean_product(Some("  ")), None);
        assert_eq!(clean_product(None), None);
        assert_eq!(
            clean_product(Some("Bitdefender Endpoint Security Tools Antimalware")),
            Some("Bitdefender Endpoint Security Tools Antimalware".to_string())
        );
    }

    #[test]
    fn bitdefender_is_not_mistaken_for_defender() {
        // "Bitdefender" enthält "defender" – die Erkennung darf das nicht verwechseln.
        assert!(!is_defender(Some(
            "Bitdefender Endpoint Security Tools Antimalware"
        )));
        assert!(is_defender(Some("Windows Defender")));
        assert!(is_defender(Some("Microsoft Defender Antivirus")));
        assert!(!is_defender(Some("G DATA Security Client")));
        assert!(!is_defender(None));
    }

    #[test]
    fn running_protection_wins_over_disabled_third_party() {
        // Bitdefender abgeschaltet, Defender übernimmt – das Gerät ist geschützt.
        let entries = [
            av(1, "Bitdefender Endpoint Security Tools Antimalware", "OFF", "Out-of-Date"),
            av(1, "Microsoft Defender Antivirus", "ON", "Up-to-Date"),
        ];

        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        runtime.block_on(async {
            let (_guard, pool) = crate::testdb::fixture().await;
            sqlx::query(
                "INSERT INTO customers (id, name, short_code, created_at) VALUES ('c1', 'Testkunde', 'TC', '2026-01-01')",
            )
            .execute(&pool)
            .await
            .unwrap();
            for sql in [
                "INSERT INTO devices (id, customer_id, ninja_device_id, hostname) VALUES ('ninja-dev-1', 'c1', 1, 'TEST-1')",
            ] {
                sqlx::query(sql).execute(&pool).await.unwrap();
            }

            persist(&pool, &[], &[], &entries).await.unwrap();

            let row: (String, String) = sqlx::query_as(
                "SELECT antivirus_status, antivirus_product FROM devices WHERE ninja_device_id = 1",
            )
            .fetch_one(&pool)
            .await
            .unwrap();

            assert_eq!(row.0, "ok");
            assert!(row.1.contains("Defender"));
        });
    }

    #[test]
    fn out_of_date_definitions_raise_a_warning() {
        let entries = [av(1, "Bitdefender", "ON", "Out-of-Date")];

        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        runtime.block_on(async {
            let (_guard, pool) = crate::testdb::fixture().await;
            sqlx::query(
                "INSERT INTO customers (id, name, short_code, created_at) VALUES ('c1', 'Testkunde', 'TC', '2026-01-01')",
            )
            .execute(&pool)
            .await
            .unwrap();
            for sql in [
                "INSERT INTO devices (id, customer_id, ninja_device_id, hostname) VALUES ('ninja-dev-1', 'c1', 1, 'TEST-1')",
            ] {
                sqlx::query(sql).execute(&pool).await.unwrap();
            }

            persist(&pool, &[], &[], &entries).await.unwrap();

            let status: String =
                sqlx::query_scalar("SELECT antivirus_status FROM devices WHERE ninja_device_id = 1")
                    .fetch_one(&pool)
                    .await
                    .unwrap();
            assert_eq!(status, "warning");
        });
    }
}
