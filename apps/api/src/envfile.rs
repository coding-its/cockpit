//! Lesen und Schreiben der `.env`.
//!
//! Die Zugangsdaten zu NinjaOne liegen bewusst nicht in der Datenbank und
//! nicht im Programm, sondern in dieser Datei. Sie muss lesbar sein, bevor
//! irgendetwas verbunden ist – und sie darf nicht im Installer stecken, damit
//! weitergegebene Pakete keine Zugänge enthalten.
//!
//! Beim Schreiben bleiben Kommentare und fremde Schlüssel erhalten: Die Datei
//! ist auch von Hand gepflegt, und ein Überschreiben würde diese Arbeit
//! zerstören.

use std::collections::BTreeMap;
use std::path::PathBuf;

/// Schreibt einen Wert so, wie dotenvy ihn wieder liest.
fn format_value(key: &str, value: &str) -> String {
    let needs_quotes = value.is_empty()
        || value
            .chars()
            .any(|c| c.is_whitespace() || c == '"' || c == '#' || c == '\'');

    if needs_quotes && !value.is_empty() {
        let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
        format!("{key}=\"{escaped}\"")
    } else {
        format!("{key}={value}")
    }
}

/// Wert ohne umschließende Anführungszeichen.
fn parse_value(raw: &str) -> String {
    let trimmed = raw.trim();

    if trimmed.len() >= 2
        && ((trimmed.starts_with('"') && trimmed.ends_with('"'))
            || (trimmed.starts_with('\'') && trimmed.ends_with('\'')))
    {
        let inner = &trimmed[1..trimmed.len() - 1];
        return inner.replace("\\\"", "\"").replace("\\\\", "\\");
    }

    trimmed.to_string()
}

/// Mögliche Orte der `.env` – dieselbe Reihenfolge wie beim Laden.
fn candidates() -> Vec<PathBuf> {
    let mut paths = vec![PathBuf::from(".env")];

    if let Some(root) = crate::dbconfig::DbConfig::path()
        .parent()
        .and_then(|data| data.parent())
    {
        let candidate = root.join(".env");
        if !paths.contains(&candidate) {
            paths.push(candidate);
        }
    }

    if let Some(dir) = crate::dbconfig::DbConfig::app_data_dir() {
        let candidate = dir.join(".env");
        if !paths.contains(&candidate) {
            paths.push(candidate);
        }
    }

    paths
}

/// Die Datei, die beim Start tatsächlich geladen wird.
pub fn path() -> PathBuf {
    candidates()
        .into_iter()
        .find(|path| path.exists())
        .unwrap_or_else(|| {
            crate::dbconfig::DbConfig::app_data_dir()
                .map(|dir| dir.join(".env"))
                .unwrap_or_else(|| PathBuf::from(".env"))
        })
}

/// Alle Schlüssel der Datei, Rohwerte ohne Anführungszeichen.
pub fn read() -> BTreeMap<String, String> {
    let path = path();
    let Ok(text) = std::fs::read_to_string(&path) else {
        return BTreeMap::new();
    };

    let mut values = BTreeMap::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let Some((key, raw)) = trimmed.split_once('=') else {
            continue;
        };

        values.insert(key.trim().to_string(), parse_value(raw));
    }

    values
}

/// Schreibt die übergebenen Werte in die `.env` und gibt den Pfad zurück.
///
/// `None` als Wert bedeutet: Schlüssel unangetastet lassen. Das ist der Weg
/// für Passwörter, die die Oberfläche nie im Klartext erhält.
pub fn write(updates: &BTreeMap<String, Option<String>>) -> std::io::Result<PathBuf> {
    let path = path();

    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }

    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    // Zeilenenden der Datei beibehalten: Windows-Editoren schreiben CRLF, und
    // ein Wechsel würde die ganze Datei als geändert erscheinen lassen.
    let newline = if existing.contains("\r\n") { "\r\n" } else { "\n" };
    let mut lines: Vec<String> = existing.lines().map(str::to_string).collect();
    let mut written: Vec<String> = Vec::new();

    for line in lines.iter_mut() {
        // Schlüssel zuerst als eigenen Wert kopieren: Sonst hält `key` die
        // Zeile geliehen, die darunter ersetzt wird.
        let Some((key, _)) = line.trim_start().split_once('=') else {
            continue;
        };
        let key = key.trim().to_string();

        if line.trim_start().starts_with('#') {
            continue;
        }

        if let Some(Some(value)) = updates.get(&key) {
            *line = format_value(&key, value);
            written.push(key);
        } else if updates.contains_key(&key) {
            // Wert bleibt (Passwort nicht überschreiben), aber als vorhanden merken.
            written.push(key);
        }
    }

    let mut appended = Vec::new();
    for (key, value) in updates {
        let Some(value) = value else { continue };
        if written.iter().any(|seen| seen == key) {
            continue;
        }
        appended.push(format_value(key, value));
    }

    if !appended.is_empty() {
        if !lines.is_empty() && !lines.last().map(|l| l.trim().is_empty()).unwrap_or(true) {
            lines.push(String::new());
        }
        lines.push("# Ergänzt über die Einstellungen".to_string());
        lines.extend(appended);
    }

    let mut text = lines.join(newline);
    if !text.ends_with(newline) {
        text.push_str(newline);
    }

    std::fs::write(&path, text)?;
    Ok(path)
}

/// Zeigt nur, ob ein Geheimnis hinterlegt ist – nie das Geheimnis selbst.
pub fn is_set(values: &BTreeMap<String, String>, key: &str) -> bool {
    values
        .get(key)
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false)
}

pub fn get<'a>(values: &'a BTreeMap<String, String>, key: &str) -> &'a str {
    values.get(key).map(String::as_str).unwrap_or("")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_with_spaces_get_quoted_and_read_back() {
        let line = format_value("NINJAONE_SCOPE", "monitoring management");
        assert_eq!(line, "NINJAONE_SCOPE=\"monitoring management\"");

        let raw = line.split_once('=').unwrap().1;
        assert_eq!(parse_value(raw), "monitoring management");
    }

    /// dotenvy bricht ohne Anführungszeichen bei Leerzeichen ab – und dann
    /// werden alle folgenden Zeilen ignoriert. Das ist der Fehler, den schon
    /// einmal stundenlang gesucht wurde.
    #[test]
    fn simple_values_stay_unquoted() {
        assert_eq!(
            format_value("NINJAONE_BASE_URL", "https://eu.ninjarmm.com"),
            "NINJAONE_BASE_URL=https://eu.ninjarmm.com"
        );
    }

    #[test]
    fn empty_value_is_written_as_empty() {
        assert_eq!(format_value("NINJAONE_CLIENT_ID", ""), "NINJAONE_CLIENT_ID=");
    }

    #[test]
    fn quotes_inside_a_value_survive() {
        let line = format_value("X", "a \"b\" c");
        let raw = line.split_once('=').unwrap().1;
        assert_eq!(parse_value(raw), "a \"b\" c");
    }
}
