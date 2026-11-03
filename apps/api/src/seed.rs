use chrono::{Duration, Local, SecondsFormat, Utc};
use sqlx::{PgPool, Postgres, Transaction};

pub async fn ensure(pool: &PgPool) -> anyhow::Result<()> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM customers")
        .fetch_one(pool)
        .await?;

    if count > 0 {
        return Ok(());
    }

    let mut tx = pool.begin().await?;
    seed(&mut tx).await?;
    tx.commit().await?;
    Ok(())
}

async fn seed(tx: &mut Transaction<'_, Postgres>) -> anyhow::Result<()> {
    let now = now_iso();
    let period = Local::now().format("%Y-%m").to_string();
    let month_name = german_month(Local::now().format("%m").to_string().parse().unwrap_or(10));
    let run_title = format!("Monatliche Wartung {month_name}");
    let today = Local::now().date_naive();

    let customer_data = [
        (
            "cust-nordlicht",
            "Nordlicht Dental GmbH",
            "NDL",
            101,
            "Dr. Anna Petersen",
            "a.petersen@nordlicht-dental.de",
            "+49 431 555 120",
            "Zahnarztpraxis mit zwei Standorten. Praxissoftware ist an den Server gebunden, Neustarts nur nach Absprache.",
        ),
        (
            "cust-weber",
            "Weber & Söhne Steuerberater",
            "WBS",
            102,
            "Jonas Weber",
            "jonas.weber@weber-steuer.de",
            "+49 40 987 654",
            "Steuerkanzlei mit DATEV-Arbeitsplätzen. Hohe Anforderungen an Datenschutz und Verfügbarkeit während der Steuerfristen.",
        ),
        (
            "cust-krueger",
            "Autohaus Krüger GmbH",
            "AKG",
            103,
            "Sabine Krüger",
            "it@autohaus-krueger.de",
            "+49 451 332 900",
            "Autohaus mit DMS und Kassensystem. Wartung außerhalb der Öffnungszeiten.",
        ),
        (
            "cust-kita",
            "Kita Sonnenschein e.V.",
            "KIS",
            104,
            "Miriam Stolz",
            "miriam.stolz@kita-sonnenschein.de",
            "+49 381 444 210",
            "Kleiner Verein mit zwei Standorten. Website enthält das Spendenformular, Ausfälle sind besonders sensibel.",
        ),
    ];

    for customer in customer_data {
        insert_customer(tx, customer, &now).await?;
    }

    let devices = [
        ("dev-ndl-srv01", "cust-nordlicht", 101001, "NDL-SRV01", "Windows Server 2022", "server", "pending", "ok", true),
        ("dev-ndl-pc01", "cust-nordlicht", 101002, "NDL-PC01", "Windows 11 Pro", "workstation", "ok", "ok", true),
        ("dev-ndl-pc02", "cust-nordlicht", 101003, "NDL-PC02", "Windows 11 Pro", "workstation", "ok", "ok", true),
        ("dev-ndl-lt01", "cust-nordlicht", 101004, "NDL-LT01", "Windows 11 Pro", "workstation", "ok", "warning", true),
        ("dev-wbs-srv01", "cust-weber", 102001, "WBS-SRV01", "Windows Server 2019", "server", "pending", "ok", true),
        ("dev-wbs-pc01", "cust-weber", 102002, "WBS-PC01", "Windows 11 Pro", "workstation", "ok", "ok", true),
        ("dev-wbs-pc02", "cust-weber", 102003, "WBS-PC02", "Windows 11 Pro", "workstation", "ok", "ok", true),
        ("dev-akg-srv01", "cust-krueger", 103001, "AKG-SRV01", "Windows Server 2022", "server", "pending", "ok", true),
        ("dev-akg-pc01", "cust-krueger", 103002, "AKG-PC01", "Windows 11 Pro", "workstation", "ok", "ok", true),
        ("dev-akg-pc02", "cust-krueger", 103003, "AKG-PC02", "Windows 11 Pro", "workstation", "ok", "ok", true),
        ("dev-akg-pc03", "cust-krueger", 103004, "AKG-PC03", "Windows 11 Pro", "workstation", "ok", "ok", true),
        ("dev-kis-lt01", "cust-kita", 104001, "KIS-LT01", "Windows 11 Pro", "workstation", "ok", "ok", true),
        ("dev-kis-pc01", "cust-kita", 104002, "KIS-PC01", "Windows 11 Pro", "workstation", "ok", "ok", true),
    ];

    for (id, customer_id, ninja_id, hostname, os, device_type, patch_status, antivirus_status, online) in devices {
        insert_device(
            tx,
            id,
            customer_id,
            ninja_id,
            hostname,
            os,
            device_type,
            &now_minus(12),
            patch_status,
            antivirus_status,
            online,
        )
        .await?;
    }

    let sites = [
        (
            "site-nordlicht-web", "cust-nordlicht", "https://www.nordlicht-dental.de", "Hauptseite",
            "rest", "6.6.2", "8.2", 4, false, "healthy", 2, 6,
            None,
        ),
        (
            "site-nordlicht-karriere", "cust-nordlicht", "https://karriere.nordlicht-dental.de", "Karriere",
            "rest", "6.6.2", "8.2", 0, false, "healthy", 2, 6,
            Some("Statische Karriereseite, keine Formulare."),
        ),
        (
            "site-weber", "cust-weber", "https://www.weber-steuer.de", "Kanzlei-Website",
            "rest", "6.5.5", "8.1", 2, true, "warning", 8, 30,
            Some("Core-Update ausstehend. Backup älter als 24 Stunden."),
        ),
        (
            "site-krueger", "cust-krueger", "https://www.autohaus-krueger.de", "Fahrzeugbestand",
            "rest", "6.6.2", "8.2", 0, false, "healthy", 1, 4,
            None,
        ),
        (
            "site-kita", "cust-kita", "https://www.kita-sonnenschein.de", "Vereinsseite + Spendenformular",
            "wpcli", "6.4.3", "8.0", 5, true, "critical", 26, 52,
            Some("5 Plugin-Updates, Core-Update, Backup 52 h alt. Updates bis zur Klärung gesperrt."),
        ),
    ];

    for site in sites {
        insert_site(tx, site).await?;
    }

    let contracts = [
        ("contract-nordlicht", "cust-nordlicht", "Wartungspaket Premium", 8.0, 95.0, 450.0, 1),
        ("contract-weber", "cust-weber", "IT-Wartung Kanzlei", 5.0, 110.0, 320.0, 1),
        ("contract-krueger", "cust-krueger", "Systemhaus-Wartung", 6.0, 89.0, 380.0, 1),
        ("contract-kita", "cust-kita", "Vereinspaket", 4.0, 85.0, 180.0, 1),
    ];

    for contract in contracts {
        insert_contract(tx, contract).await?;
    }

    let run_nordlicht = format!("run-nordlicht-{period}");
    let run_weber = format!("run-weber-{period}");
    let run_krueger = format!("run-krueger-{period}");
    let run_kita = format!("run-kita-{period}");

    insert_run(
        tx,
        &run_nordlicht,
        "cust-nordlicht",
        Some("contract-nordlicht"),
        &period,
        &run_title,
        &local_datetime(&today, -2, 18, 0),
        "in_progress",
        Some("Max Mustermann"),
        Some("Server benötigt nach der Patchinstallation einen Neustart. Praxis schließt um 18:00 Uhr."),
        &now,
        None,
    )
    .await?;

    insert_run(
        tx,
        &run_weber,
        "cust-weber",
        Some("contract-weber"),
        &period,
        &run_title,
        &local_datetime(&today, 2, 17, 30),
        "planned",
        Some("Max Mustermann"),
        Some("Core-Update erst nach verifiziertem Backup und Freigabe durch die Kanzlei."),
        &now,
        None,
    )
    .await?;

    insert_run(
        tx,
        &run_krueger,
        "cust-krueger",
        Some("contract-krueger"),
        &period,
        &run_title,
        &local_datetime(&today, -14, 18, 0),
        "done",
        Some("M. Becker"),
        Some("Außerhalb der Öffnungszeiten durchgeführt. Kassensystem vorher gestoppt."),
        &now,
        Some(local_datetime(&today, -14, 21, 10)),
    )
    .await?;

    insert_run(
        tx,
        &run_kita,
        "cust-kita",
        Some("contract-kita"),
        &period,
        &run_title,
        &local_datetime(&today, 4, 16, 0),
        "blocked",
        Some("Max Mustermann"),
        Some("Backup ist 52 Stunden alt. Updates sind bis zur Sicherung und Freigabe gesperrt."),
        &now,
        None,
    )
    .await?;

    let task_nordlicht_scan = seed_nordlicht(tx, &run_nordlicht, &today).await?;
    seed_weber(tx, &run_weber, &today).await?;
    seed_krueger(tx, &run_krueger, &today).await?;
    seed_kita(tx, &run_kita, &today).await?;

    insert_execution(
        tx,
        "exec-nordlicht-scan",
        &task_nordlicht_scan,
        "succeeded",
        "Scan abgeschlossen: 3 Updates ausstehend (2× Windows, 1× Treiber).",
        2410,
        &now_minus(2880),
    )
    .await?;

    insert_job(
        tx,
        "job-nordlicht-scan",
        Some(&task_nordlicht_scan),
        "NINJA_PATCH",
        "succeeded",
        100,
        "Scan abgeschlossen: 3 Updates ausstehend.",
        &now_minus(2880),
    )
    .await?;

    seed_time_entries(tx, &run_nordlicht, &run_weber, &run_krueger, &run_kita, &now).await?;
    seed_audit(tx).await?;

    sqlx::query("INSERT INTO app_settings (key, value) VALUES ('ninjaone.last_sync', $1)")
        .bind(now_minus(65))
        .execute(&mut **tx)
        .await?;

    Ok(())
}

async fn seed_nordlicht(
    tx: &mut Transaction<'_, Postgres>,
    run_id: &str,
    _today: &chrono::NaiveDate,
) -> anyhow::Result<String> {
    let scan_id = format!("task-ndl-scan-{run_id}");
    let tasks = vec![
        TaskSeed {
            id: scan_id.clone(),
            run_id,
            position: 1,
            title: "Patch-Scan Serversystem",
            description: Some("Windows- und Software-Patches auf NDL-SRV01 prüfen."),
            kind: "NINJA_PATCH",
            target_type: Some("device"),
            target_id: Some("dev-ndl-srv01"),
            target_label: Some("NDL-SRV01 · Windows Server 2022"),
            status: "done",
            risk: "low",
            requires_approval: false,
            approved: false,
            result_summary: Some("Scan abgeschlossen: 3 Updates ausstehend (2× Windows, 1× Treiber)."),
            started_at: Some(now_minus(2890)),
            finished_at: Some(now_minus(2885)),
        },
        TaskSeed {
            id: format!("task-ndl-apply-{run_id}"),
            run_id,
            position: 2,
            title: "Sicherheitsupdates installieren",
            description: Some("Freigegebene Patches installieren, Neustart nur im Wartungsfenster."),
            kind: "NINJA_PATCH",
            target_type: Some("device"),
            target_id: Some("dev-ndl-srv01"),
            target_label: Some("NDL-SRV01 · Windows Server 2022"),
            status: "open",
            risk: "medium",
            requires_approval: false,
            approved: false,
            result_summary: None,
            started_at: None,
            finished_at: None,
        },
        TaskSeed {
            id: format!("task-ndl-reboot-{run_id}"),
            run_id,
            position: 3,
            title: "Neustartstatus prüfen",
            description: Some("Script reboot-check ausführen und Praxisleitung informieren."),
            kind: "NINJA_SCRIPT",
            target_type: Some("device"),
            target_id: Some("dev-ndl-srv01"),
            target_label: Some("NDL-SRV01 · Windows Server 2022"),
            status: "open",
            risk: "low",
            requires_approval: false,
            approved: false,
            result_summary: None,
            started_at: None,
            finished_at: None,
        },
        TaskSeed {
            id: format!("task-ndl-wp-{run_id}"),
            run_id,
            position: 4,
            title: "WordPress-Plugins aktualisieren",
            description: Some("4 Updates: Yoast SEO, Contact Form 7, Wordfence, WP Mail SMTP."),
            kind: "WP_PLUGIN_UPDATE",
            target_type: Some("site"),
            target_id: Some("site-nordlicht-web"),
            target_label: Some("nordlicht-dental.de"),
            status: "open",
            risk: "medium",
            requires_approval: false,
            approved: false,
            result_summary: None,
            started_at: None,
            finished_at: None,
        },
        TaskSeed {
            id: format!("task-ndl-core-{run_id}"),
            run_id,
            position: 5,
            title: "WordPress-Core prüfen",
            description: Some("Aktuelle Core-Version und Update-Status verifizieren."),
            kind: "WP_CORE_UPDATE",
            target_type: Some("site"),
            target_id: Some("site-nordlicht-web"),
            target_label: Some("nordlicht-dental.de"),
            status: "done",
            risk: "low",
            requires_approval: false,
            approved: false,
            result_summary: Some("Kein Core-Update verfügbar. Installation ist auf 6.6.2."),
            started_at: Some(now_minus(2870)),
            finished_at: Some(now_minus(2860)),
        },
        TaskSeed {
            id: format!("task-ndl-av-{run_id}"),
            run_id,
            position: 6,
            title: "Antivirus-Status prüfen",
            description: Some("Definitionsstand und Bedrohungen auf allen vier Geräten kontrollieren."),
            kind: "MANUAL",
            target_type: Some("customer"),
            target_id: Some("cust-nordlicht"),
            target_label: Some("4 Geräte"),
            status: "done",
            risk: "low",
            requires_approval: false,
            approved: false,
            result_summary: Some("Alle Geräte: Definitionen aktuell, keine Bedrohungen. NDL-LT01 war zuletzt offline."),
            started_at: Some(now_minus(2850)),
            finished_at: Some(now_minus(2840)),
        },
    ];

    for task in tasks {
        insert_task(tx, &task).await?;
    }

    Ok(scan_id)
}

async fn seed_weber(
    tx: &mut Transaction<'_, Postgres>,
    run_id: &str,
    _today: &chrono::NaiveDate,
) -> anyhow::Result<()> {
    let tasks = vec![
        TaskSeed {
            id: format!("task-wbs-scan-{run_id}"),
            run_id,
            position: 1,
            title: "Patch-Scan Serversystem",
            description: Some("Patchstand auf WBS-SRV01 prüfen."),
            kind: "NINJA_PATCH",
            target_type: Some("device"),
            target_id: Some("dev-wbs-srv01"),
            target_label: Some("WBS-SRV01 · Windows Server 2019"),
            status: "open",
            risk: "low",
            requires_approval: false,
            approved: false,
            result_summary: None,
            started_at: None,
            finished_at: None,
        },
        TaskSeed {
            id: format!("task-wbs-backup-{run_id}"),
            run_id,
            position: 2,
            title: "Backup verifizieren",
            description: Some("Letztes Backup prüfen. Voraussetzung für das Core-Update."),
            kind: "MANUAL",
            target_type: Some("site"),
            target_id: Some("site-weber"),
            target_label: Some("weber-steuer.de"),
            status: "open",
            risk: "medium",
            requires_approval: false,
            approved: false,
            result_summary: None,
            started_at: None,
            finished_at: None,
        },
        TaskSeed {
            id: format!("task-wbs-core-{run_id}"),
            run_id,
            position: 3,
            title: "WordPress-Core auf 6.6.2 aktualisieren",
            description: Some("Major-Sprung von 6.5.5. Plugin-Kompatibilität und Backup vorher prüfen."),
            kind: "WP_CORE_UPDATE",
            target_type: Some("site"),
            target_id: Some("site-weber"),
            target_label: Some("weber-steuer.de"),
            status: "open",
            risk: "high",
            requires_approval: true,
            approved: false,
            result_summary: None,
            started_at: None,
            finished_at: None,
        },
        TaskSeed {
            id: format!("task-wbs-plugins-{run_id}"),
            run_id,
            position: 4,
            title: "WordPress-Plugins aktualisieren",
            description: Some("2 Updates nach dem Core-Update einspielen."),
            kind: "WP_PLUGIN_UPDATE",
            target_type: Some("site"),
            target_id: Some("site-weber"),
            target_label: Some("weber-steuer.de"),
            status: "open",
            risk: "medium",
            requires_approval: false,
            approved: false,
            result_summary: None,
            started_at: None,
            finished_at: None,
        },
        TaskSeed {
            id: format!("task-wbs-health-{run_id}"),
            run_id,
            position: 5,
            title: "Health-Check nach Updates",
            description: Some("Startseite, Login und Formular testen; PHP-Fehlerlog prüfen."),
            kind: "WP_HEALTH",
            target_type: Some("site"),
            target_id: Some("site-weber"),
            target_label: Some("weber-steuer.de"),
            status: "open",
            risk: "low",
            requires_approval: false,
            approved: false,
            result_summary: None,
            started_at: None,
            finished_at: None,
        },
    ];

    for task in tasks {
        insert_task(tx, &task).await?;
    }
    Ok(())
}

async fn seed_krueger(
    tx: &mut Transaction<'_, Postgres>,
    run_id: &str,
    today: &chrono::NaiveDate,
) -> anyhow::Result<()> {
    let tasks = vec![
        TaskSeed {
            id: format!("task-akg-scan-{run_id}"),
            run_id,
            position: 1,
            title: "Patch-Scan Serversystem",
            description: Some("Patchstand auf AKG-SRV01 prüfen."),
            kind: "NINJA_PATCH",
            target_type: Some("device"),
            target_id: Some("dev-akg-srv01"),
            target_label: Some("AKG-SRV01 · Windows Server 2022"),
            status: "done",
            risk: "low",
            requires_approval: false,
            approved: false,
            result_summary: Some("Scan abgeschlossen: 4 Updates ausstehend."),
            started_at: Some(local_datetime(today, -14, 18, 4)),
            finished_at: Some(local_datetime(today, -14, 18, 9)),
        },
        TaskSeed {
            id: format!("task-akg-apply-{run_id}"),
            run_id,
            position: 2,
            title: "Patches installieren",
            description: Some("Alle ausstehenden Updates installieren, DMS vorher stoppen."),
            kind: "NINJA_PATCH",
            target_type: Some("device"),
            target_id: Some("dev-akg-srv01"),
            target_label: Some("AKG-SRV01 · Windows Server 2022"),
            status: "done",
            risk: "medium",
            requires_approval: false,
            approved: true,
            result_summary: Some("4 Updates installiert, 0 ausstehend. Neustart um 19:42 Uhr erfolgreich."),
            started_at: Some(local_datetime(today, -14, 18, 15)),
            finished_at: Some(local_datetime(today, -14, 19, 45)),
        },
        TaskSeed {
            id: format!("task-akg-wp-{run_id}"),
            run_id,
            position: 3,
            title: "WordPress-Plugins aktualisieren",
            description: Some("2 Plugin-Updates einspielen, Fahrzeugbestand danach prüfen."),
            kind: "WP_PLUGIN_UPDATE",
            target_type: Some("site"),
            target_id: Some("site-krueger"),
            target_label: Some("autohaus-krueger.de"),
            status: "done",
            risk: "medium",
            requires_approval: false,
            approved: false,
            result_summary: Some("2 Plugins aktualisiert. Fahrzeugbestand und Kontaktformular geprüft."),
            started_at: Some(local_datetime(today, -14, 19, 50)),
            finished_at: Some(local_datetime(today, -14, 20, 5)),
        },
        TaskSeed {
            id: format!("task-akg-manual-{run_id}"),
            run_id,
            position: 4,
            title: "Sichtprüfung & Dokumentation",
            description: Some("Kassensystem und DMS prüfen, Wartungsprotokoll ablegen."),
            kind: "MANUAL",
            target_type: Some("customer"),
            target_id: Some("cust-krueger"),
            target_label: Some("Autohaus Krüger"),
            status: "done",
            risk: "low",
            requires_approval: false,
            approved: false,
            result_summary: Some("Kassensystem und DMS geprüft, keine Auffälligkeiten. Protokoll im Kundenordner."),
            started_at: Some(local_datetime(today, -14, 20, 10)),
            finished_at: Some(local_datetime(today, -14, 21, 5)),
        },
    ];

    for task in tasks {
        insert_task(tx, &task).await?;
    }
    Ok(())
}

async fn seed_kita(
    tx: &mut Transaction<'_, Postgres>,
    run_id: &str,
    _today: &chrono::NaiveDate,
) -> anyhow::Result<()> {
    let tasks = vec![
        TaskSeed {
            id: format!("task-kis-backup-{run_id}"),
            run_id,
            position: 1,
            title: "Backup-Alter prüfen",
            description: Some("Backup ist 52 Stunden alt. Mindestanforderung vor Updates: 24 Stunden."),
            kind: "MANUAL",
            target_type: Some("site"),
            target_id: Some("site-kita"),
            target_label: Some("kita-sonnenschein.de"),
            status: "open",
            risk: "high",
            requires_approval: false,
            approved: false,
            result_summary: None,
            started_at: None,
            finished_at: None,
        },
        TaskSeed {
            id: format!("task-kis-plugins-{run_id}"),
            run_id,
            position: 2,
            title: "WordPress-Plugins aktualisieren",
            description: Some("5 Updates inklusive Formular-Plugin. Erst nach verifiziertem Backup."),
            kind: "WP_PLUGIN_UPDATE",
            target_type: Some("site"),
            target_id: Some("site-kita"),
            target_label: Some("kita-sonnenschein.de"),
            status: "open",
            risk: "high",
            requires_approval: true,
            approved: false,
            result_summary: None,
            started_at: None,
            finished_at: None,
        },
        TaskSeed {
            id: format!("task-kis-core-{run_id}"),
            run_id,
            position: 3,
            title: "WordPress-Core aktualisieren",
            description: Some("6.4.3 auf 6.6.2. Spendenformular und Vereinsseite anschließend testen."),
            kind: "WP_CORE_UPDATE",
            target_type: Some("site"),
            target_id: Some("site-kita"),
            target_label: Some("kita-sonnenschein.de"),
            status: "open",
            risk: "high",
            requires_approval: true,
            approved: false,
            result_summary: None,
            started_at: None,
            finished_at: None,
        },
        TaskSeed {
            id: format!("task-kis-health-{run_id}"),
            run_id,
            position: 4,
            title: "Health-Check nach Updates",
            description: Some("Startseite, Spendenformular und Login prüfen."),
            kind: "WP_HEALTH",
            target_type: Some("site"),
            target_id: Some("site-kita"),
            target_label: Some("kita-sonnenschein.de"),
            status: "open",
            risk: "low",
            requires_approval: false,
            approved: false,
            result_summary: None,
            started_at: None,
            finished_at: None,
        },
    ];

    for task in tasks {
        insert_task(tx, &task).await?;
    }
    Ok(())
}

async fn seed_time_entries(
    tx: &mut Transaction<'_, Postgres>,
    run_nordlicht: &str,
    run_weber: &str,
    run_krueger: &str,
    run_kita: &str,
    now: &str,
) -> anyhow::Result<()> {
    let scan_task_id = format!("task-ndl-scan-{run_nordlicht}");
    let plugin_task_id = format!("task-ndl-wp-{run_nordlicht}");

    let entries: Vec<(&str, &str, Option<&str>, Option<&str>, &str, &str, String, i64, bool, &str)> = vec![
        (
            "time-1", "cust-nordlicht", Some(run_nordlicht), Some(scan_task_id.as_str()),
            "Patchmanagement & Serverwartung",
            "Scan, Patchfreigabe vorbereitet, Neustart mit Praxisleitung abgestimmt.",
            now_minus(2890), 5400, true, "timer",
        ),
        (
            "time-2", "cust-nordlicht", Some(run_nordlicht), None,
            "Dokumentation",
            "Wartungsprotokoll und Hinweise für die Praxis ergänzt.",
            now_minus(2830), 1800, false, "manual",
        ),
        (
            "time-3", "cust-nordlicht", Some(run_nordlicht), Some(plugin_task_id.as_str()),
            "WordPress-Pflege",
            "Plugin-Updates vorbereitet, Kompatibilität mit dem Theme geprüft.",
            now_minus(1440), 7200, true, "timer",
        ),
        (
            "time-4", "cust-nordlicht", Some(run_nordlicht), Some(scan_task_id.as_str()),
            "NinjaOne Patch-Job (Maschinenzeit)",
            "Automatisch erfasste Laufzeit des Scan-Jobs.",
            now_minus(2870), 1500, false, "machine",
        ),
        (
            "time-5", "cust-weber", Some(run_weber), None,
            "Patchmanagement",
            "Patchstand geprüft und Update-Plan für die Kanzlei abgestimmt.",
            now_minus(7200), 10800, true, "timer",
        ),
        (
            "time-6", "cust-weber", Some(run_weber), None,
            "Backup-Verifikation",
            "Backup geprüft, Wiederherstellungstest dokumentiert.",
            now_minus(5760), 2700, true, "manual",
        ),
        (
            "time-7", "cust-krueger", Some(run_krueger), None,
            "Monatliche Wartung",
            "Patches, WordPress-Updates und Sichtprüfung außerhalb der Öffnungszeiten.",
            now_minus(20160), 11700, true, "timer",
        ),
        (
            "time-8", "cust-krueger", Some(run_krueger), None,
            "NinjaOne Patch-Job (Maschinenzeit)",
            "Automatisch erfasste Laufzeit der Patchinstallation.",
            now_minus(20140), 1200, false, "machine",
        ),
        (
            "time-9", "cust-kita", Some(run_kita), None,
            "WordPress-Analyse",
            "Plugin- und Core-Updates analysiert, Backup-Thema mit dem Verein besprochen.",
            now_minus(11520), 3600, true, "timer",
        ),
        (
            "time-10", "cust-kita", Some(run_kita), None,
            "WP-CLI Inventar (Maschinenzeit)",
            "Inventarabfrage über WP-CLI.",
            now_minus(11510), 900, false, "machine",
        ),
    ];

    for (id, customer_id, run_id, task_id, activity, note, started_at, duration, billable, source) in entries {
        let ended_at = plus_seconds(&started_at, duration);
        insert_time_entry(
            tx,
            id,
            customer_id,
            run_id,
            task_id,
            "Max Mustermann",
            activity,
            note,
            &started_at,
            Some(&ended_at),
            duration,
            billable,
            source,
            now,
        )
        .await?;
    }

    Ok(())
}

async fn seed_audit(tx: &mut Transaction<'_, Postgres>) -> anyhow::Result<()> {
    let events = [
        ("audit-1", "System", "NinjaOne synchronisiert", Some("integration"), None, "4 Organisationen und 13 Geräte abgeglichen (Mock-Modus).", now_minus(65)),
        ("audit-2", "Max Mustermann", "Wartungslauf gestartet", Some("run"), Some("run-nordlicht"), "Wartungsfenster Nordlicht Dental geöffnet.", now_minus(2890)),
        ("audit-3", "M. Becker", "Plugin-Update ausgeführt", Some("site"), Some("site-krueger"), "2 WordPress-Plugins aktualisiert.", now_minus(20100)),
        ("audit-4", "M. Becker", "Wartungslauf abgeschlossen", Some("run"), Some("run-krueger"), "Alle Aufgaben erledigt, Protokoll erzeugt.", now_minus(20000)),
        ("audit-5", "System", "Backup-Warnung", Some("site"), Some("site-kita"), "Backup ist 52 Stunden alt. Updates gesperrt.", now_minus(180)),
    ];

    for (id, actor, action, entity_type, entity_id, detail, created_at) in events {
        insert_audit(tx, id, actor, action, entity_type, entity_id, detail, &created_at).await?;
    }
    Ok(())
}

struct TaskSeed<'a> {
    id: String,
    run_id: &'a str,
    position: i64,
    title: &'a str,
    description: Option<&'a str>,
    kind: &'a str,
    target_type: Option<&'a str>,
    target_id: Option<&'a str>,
    target_label: Option<&'a str>,
    status: &'a str,
    risk: &'a str,
    requires_approval: bool,
    approved: bool,
    result_summary: Option<&'a str>,
    started_at: Option<String>,
    finished_at: Option<String>,
}

async fn insert_customer(
    tx: &mut Transaction<'_, Postgres>,
    customer: (&str, &str, &str, i64, &str, &str, &str, &str),
    now: &str,
) -> anyhow::Result<()> {
    let (id, name, short_code, ninja_org_id, contact_name, contact_email, phone, notes) = customer;
    sqlx::query(
        "INSERT INTO customers (id, name, short_code, ninja_org_id, contact_name, contact_email, phone, notes, active, created_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, true, $9)",
    )
    .bind(id)
    .bind(name)
    .bind(short_code)
    .bind(ninja_org_id)
    .bind(contact_name)
    .bind(contact_email)
    .bind(phone)
    .bind(notes)
    .bind(now)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn insert_device(
    tx: &mut Transaction<'_, Postgres>,
    id: &str,
    customer_id: &str,
    ninja_device_id: i64,
    hostname: &str,
    os: &str,
    device_type: &str,
    last_seen: &str,
    patch_status: &str,
    antivirus_status: &str,
    online: bool,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO devices (id, customer_id, ninja_device_id, hostname, os, device_type, last_seen, patch_status, antivirus_status, online)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
    )
    .bind(id)
    .bind(customer_id)
    .bind(ninja_device_id)
    .bind(hostname)
    .bind(os)
    .bind(device_type)
    .bind(last_seen)
    .bind(patch_status)
    .bind(antivirus_status)
    .bind(online)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn insert_site(
    tx: &mut Transaction<'_, Postgres>,
    site: (&str, &str, &str, &str, &str, &str, &str, i64, bool, &str, i64, i64, Option<&str>),
) -> anyhow::Result<()> {
    let (id, customer_id, url, label, agent_mode, wp_version, php_version, plugin_updates, core_update, status, checked_hours_ago, backup_age_hours, notes) = site;
    let checked = now_minus(checked_hours_ago * 60);
    sqlx::query(
        "INSERT INTO sites (id, customer_id, url, label, environment, agent_mode, wp_version, php_version, plugin_updates, core_update, status, last_checked, backup_age_hours, notes)
         VALUES ($1, $2, $3, $4, 'prod', $5, $6, $7, $8, $9, $10, $11, $12, $13)",
    )
    .bind(id)
    .bind(customer_id)
    .bind(url)
    .bind(label)
    .bind(agent_mode)
    .bind(wp_version)
    .bind(php_version)
    .bind(plugin_updates)
    .bind(core_update)
    .bind(status)
    .bind(checked)
    .bind(backup_age_hours)
    .bind(notes)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn insert_contract(
    tx: &mut Transaction<'_, Postgres>,
    contract: (&str, &str, &str, f64, f64, f64, i64),
) -> anyhow::Result<()> {
    let (id, customer_id, name, included_hours, hourly_rate, flat_fee, billing_day) = contract;
    sqlx::query(
        "INSERT INTO contracts (id, customer_id, name, interval, included_hours, hourly_rate, flat_fee, billing_day, active)
         VALUES ($1, $2, $3, 'monthly', $4, $5, $6, $7, true)",
    )
    .bind(id)
    .bind(customer_id)
    .bind(name)
    .bind(included_hours)
    .bind(hourly_rate)
    .bind(flat_fee)
    .bind(billing_day)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn insert_run(
    tx: &mut Transaction<'_, Postgres>,
    id: &str,
    customer_id: &str,
    contract_id: Option<&str>,
    period: &str,
    title: &str,
    scheduled_for: &str,
    status: &str,
    assignee: Option<&str>,
    notes: Option<&str>,
    created_at: &str,
    completed_at: Option<String>,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO runs (id, customer_id, contract_id, period, title, scheduled_for, status, assignee, notes, created_at, completed_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
    )
    .bind(id)
    .bind(customer_id)
    .bind(contract_id)
    .bind(period)
    .bind(title)
    .bind(scheduled_for)
    .bind(status)
    .bind(assignee)
    .bind(notes)
    .bind(created_at)
    .bind(completed_at.as_deref())
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn insert_task(
    tx: &mut Transaction<'_, Postgres>,
    task: &TaskSeed<'_>,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO tasks (id, run_id, position, title, description, kind, target_type, target_id, target_label, status, risk, requires_approval, approved, result_summary, started_at, finished_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16)",
    )
    .bind(&task.id)
    .bind(task.run_id)
    .bind(task.position)
    .bind(task.title)
    .bind(task.description)
    .bind(task.kind)
    .bind(task.target_type)
    .bind(task.target_id)
    .bind(task.target_label)
    .bind(task.status)
    .bind(task.risk)
    .bind(task.requires_approval)
    .bind(task.approved)
    .bind(task.result_summary)
    .bind(task.started_at.as_deref())
    .bind(task.finished_at.as_deref())
    .execute(&mut **tx)
    .await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn insert_time_entry(
    tx: &mut Transaction<'_, Postgres>,
    id: &str,
    customer_id: &str,
    run_id: Option<&str>,
    task_id: Option<&str>,
    user_name: &str,
    activity: &str,
    note: &str,
    started_at: &str,
    ended_at: Option<&str>,
    duration_seconds: i64,
    billable: bool,
    source: &str,
    created_at: &str,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO time_entries (id, customer_id, run_id, task_id, user_name, activity, note, started_at, ended_at, duration_seconds, billable, source, created_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)",
    )
    .bind(id)
    .bind(customer_id)
    .bind(run_id)
    .bind(task_id)
    .bind(user_name)
    .bind(activity)
    .bind(note)
    .bind(started_at)
    .bind(ended_at)
    .bind(duration_seconds)
    .bind(billable)
    .bind(source)
    .bind(created_at)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn insert_execution(
    tx: &mut Transaction<'_, Postgres>,
    id: &str,
    task_id: &str,
    status: &str,
    response: &str,
    duration_ms: i64,
    created_at: &str,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO task_executions (id, task_id, status, response, duration_ms, created_at)
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(id)
    .bind(task_id)
    .bind(status)
    .bind(response)
    .bind(duration_ms)
    .bind(created_at)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn insert_job(
    tx: &mut Transaction<'_, Postgres>,
    id: &str,
    task_id: Option<&str>,
    kind: &str,
    status: &str,
    progress: i64,
    message: &str,
    created_at: &str,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO jobs (id, task_id, kind, status, progress, message, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $7)",
    )
    .bind(id)
    .bind(task_id)
    .bind(kind)
    .bind(status)
    .bind(progress)
    .bind(message)
    .bind(created_at)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn insert_audit(
    tx: &mut Transaction<'_, Postgres>,
    id: &str,
    actor: &str,
    action: &str,
    entity_type: Option<&str>,
    entity_id: Option<&str>,
    detail: &str,
    created_at: &str,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO audit_events (id, actor, action, entity_type, entity_id, detail, created_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(id)
    .bind(actor)
    .bind(action)
    .bind(entity_type)
    .bind(entity_id)
    .bind(detail)
    .bind(created_at)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

fn now_iso() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn now_minus(minutes: i64) -> String {
    (Utc::now() - Duration::minutes(minutes)).to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn plus_seconds(start: &str, seconds: i64) -> String {
    chrono::DateTime::parse_from_rfc3339(start)
        .map(|value| (value + Duration::seconds(seconds)).to_rfc3339_opts(SecondsFormat::Secs, true))
        .unwrap_or_else(|_| start.to_string())
}

fn local_datetime(today: &chrono::NaiveDate, days: i64, hour: u32, minute: u32) -> String {
    let date = *today + Duration::days(days);
    format!(
        "{}T{:02}:{:02}:00",
        date.format("%Y-%m-%d"),
        hour,
        minute
    )
}

fn german_month(month: u32) -> &'static str {
    [
        "Januar", "Februar", "März", "April", "Mai", "Juni", "Juli", "August", "September",
        "Oktober", "November", "Dezember",
    ]
    .get(month.saturating_sub(1) as usize)
    .copied()
    .unwrap_or("Wartung")
}
