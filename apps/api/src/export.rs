use crate::models::TimeEntryListItem;
use crate::util::{german_date, german_decimal};

pub fn to_csv(entries: &[TimeEntryListItem]) -> anyhow::Result<String> {
    let mut writer = csv::WriterBuilder::new()
        .delimiter(b';')
        .from_writer(Vec::new());

    writer.write_record([
        "Leistungsdatum",
        "Kunde",
        "Leistungsart",
        "Beschreibung",
        "Menge",
        "Einheit",
        "Einzelpreis",
        "Betrag",
        "Steuersatz",
        "Abrechenbar",
        "Quelle",
    ])?;

    for entry in entries {
        let hours = entry.duration_seconds as f64 / 3600.0;
        let amount = if entry.billable {
            hours * entry.hourly_rate
        } else {
            0.0
        };

        let record = vec![
            german_date(&entry.started_at),
            entry.customer_name.clone(),
            entry.activity.clone(),
            entry.note.clone().unwrap_or_default(),
            german_decimal(hours, 2),
            "Stunde".to_string(),
            german_decimal(entry.hourly_rate, 2),
            german_decimal(amount, 2),
            "19".to_string(),
            if entry.billable { "ja" } else { "nein" }.to_string(),
            source_label(&entry.source).to_string(),
        ];

        writer.write_record(&record)?;
    }

    let bytes = writer
        .into_inner()
        .map_err(|error| anyhow::anyhow!("CSV konnte nicht erzeugt werden: {error}"))?;
    let mut output = String::from('\u{feff}');
    output.push_str(&String::from_utf8(bytes)?);
    Ok(output)
}

pub fn filename(month: &str) -> String {
    format!("zeiten-{month}.csv")
}

fn source_label(source: &str) -> &str {
    match source {
        "timer" => "Timer",
        "manual" => "manuell",
        "machine" => "Maschinenzeit",
        _ => source,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry() -> TimeEntryListItem {
        TimeEntryListItem {
            id: "time-1".to_string(),
            customer_id: Some("cust-1".to_string()),
            web_client_id: None,
            owner_name: "Muster & Söhne".to_string(),
            owner_short_code: "MUS".to_string(),
            site_label: None,
            device_label: None,
            customer_name: "Muster & Söhne".to_string(),
            customer_short_code: "MUS".to_string(),
            run_id: None,
            task_id: None,
            task_title: None,
            user_name: "Max Mustermann".to_string(),
            activity: "Patchmanagement".to_string(),
            note: Some("Server; Neustart".to_string()),
            started_at: "2026-10-20T09:00:00+02:00".to_string(),
            ended_at: Some("2026-10-20T10:30:00+02:00".to_string()),
            duration_seconds: 5400,
            billable: true,
            source: "timer".to_string(),
            hourly_rate: 95.0,
        }
    }

    #[test]
    fn csv_uses_semicolon_and_german_numbers() {
        let csv = to_csv(&[entry()]).unwrap();
        assert!(csv.starts_with('\u{feff}'));
        assert!(csv.contains("20.10.2026"));
        assert!(csv.contains("1,50"));
        assert!(csv.contains("142,50"));
        assert!(csv.contains("Muster & Söhne"));
    }
}
