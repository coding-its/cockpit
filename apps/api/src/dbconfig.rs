use std::env;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Verbindung zur PostgreSQL-Datenbank.
///
/// Diese Werte stehen bewusst **nicht** in der Datenbank: Man muss sich
/// verbinden können, bevor man dort etwas lesen kann. Sie liegen in einer
/// kleinen Datei neben der Anwendung (`data/db.json`), damit sich der
/// Datenbankserver später umziehen lässt, ohne das Programm neu zu bauen.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DbConfig {
    pub host: String,
    pub port: u16,
    pub database: String,
    pub user: String,
    pub password: String,
}

impl Default for DbConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 5432,
            database: "wartungscockpit".to_string(),
            user: "cockpit".to_string(),
            password: String::new(),
        }
    }
}

impl DbConfig {
    /// Ordner für installationsweite Einstellungen: `%APPDATA%\Wartungscockpit`.
    ///
    /// Eine MSI-Installation liegt unter `Program Files` – dort darf niemand
    /// schreiben. Ohne diesen Ort fände die installierte App weder ihre
    /// Datenbankverbindung noch die NinjaOne-Zugänge.
    pub fn app_data_dir() -> Option<PathBuf> {
        let base = env::var("APPDATA")
            .or_else(|_| env::var("XDG_CONFIG_HOME"))
            .ok()?;
        Some(PathBuf::from(base).join("Wartungscockpit"))
    }

    /// Mögliche Orte der Konfigurationsdatei, in dieser Reihenfolge.
    ///
    /// Ohne die Suche scheitert der Start, sobald das Programm aus einem
    /// anderen Arbeitsverzeichnis läuft – etwa per Verknüpfung oder aus der
    /// Tauri-Hülle, wo das Arbeitsverzeichnis die Programmdatei ist.
    fn candidates() -> Vec<PathBuf> {
        if let Ok(custom) = env::var("DB_CONFIG") {
            return vec![PathBuf::from(custom)];
        }

        let mut paths = vec![PathBuf::from("data").join("db.json")];

        if let Ok(exe) = std::env::current_exe() {
            let mut dir = exe.parent().map(PathBuf::from);
            // Sechs Ebenen reichen für `apps/api/target/debug/…` im Projekt.
            for _ in 0..6 {
                let Some(current) = dir else { break };
                let candidate = current.join("data").join("db.json");
                if !paths.contains(&candidate) {
                    paths.push(candidate);
                }
                dir = current.parent().map(PathBuf::from);
            }
        }

        if let Some(dir) = Self::app_data_dir() {
            paths.push(dir.join("db.json"));
        }

        paths
    }

    /// Ort der Konfigurationsdatei: die erste vorhandene. Gibt es noch keine,
    /// wird der Programm-Datenordner benutzt – dorthin darf geschrieben
    /// werden, im Gegensatz zu `Program Files`.
    pub fn path() -> PathBuf {
        let paths = Self::candidates();

        paths
            .iter()
            .find(|path| path.exists())
            .cloned()
            .unwrap_or_else(|| {
                paths
                    .last()
                    .cloned()
                    .unwrap_or_else(|| PathBuf::from("data/db.json"))
            })
    }

    /// Liest die Verbindung: Datei, sonst `DATABASE_URL`, sonst Vorgaben.
    pub fn load() -> (Self, Option<String>) {
        if let Ok(raw) = env::var("DATABASE_URL") {
            if raw.starts_with("postgres") {
                if let Some(parsed) = Self::from_url(&raw) {
                    return (parsed, Some("DATABASE_URL".to_string()));
                }
            }
        }

        for path in Self::candidates() {
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };

            match serde_json::from_str::<Self>(&text) {
                Ok(config) => return (config, Some(path.display().to_string())),
                Err(error) => {
                    tracing::warn!(%error, path = %path.display(), "Datenbankkonfiguration unlesbar");
                    return (Self::default(), None);
                }
            }
        }

        (Self::default(), None)
    }

    /// Schreibt die Verbindung zurück, legt den Ordner bei Bedarf an.
    pub fn save(&self) -> std::io::Result<PathBuf> {
        let path = Self::path();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let text = serde_json::to_string_pretty(self)
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
        std::fs::write(&path, text)?;
        Ok(path)
    }

    /// Verbindungszeichenkette für den Treiber. Benutzername und Passwort
    /// werden prozentkodiert, weil Sonderzeichen die URL sonst zerlegen.
    pub fn url(&self) -> String {
        format!(
            "postgres://{}:{}@{}:{}/{}",
            encode(&self.user),
            encode(&self.password),
            self.host,
            self.port,
            encode(&self.database)
        )
    }

    fn from_url(raw: &str) -> Option<Self> {
        let rest = raw.strip_prefix("postgres://")?;
        let (credentials, hostpath) = rest.split_once('@')?;
        let (user, password) = credentials.split_once(':').unwrap_or((credentials, ""));
        let (hostport, database) = hostpath.split_once('/')?;
        let (host, port) = match hostport.split_once(':') {
            Some((host, port)) => (host.to_string(), port.parse().ok()?),
            None => (hostport.to_string(), 5432),
        };

        Some(Self {
            host,
            port,
            database: decode(database.split('?').next().unwrap_or(database)),
            user: decode(user),
            password: decode(password),
        })
    }

    /// Nie das Passwort im Klartext ins Log oder an die Oberfläche geben.
    pub fn describe(&self) -> String {
        format!("{}@{}:{}/{}", self.user, self.host, self.port, self.database)
    }
}

fn encode(value: &str) -> String {
    value
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                (byte as char).to_string()
            }
            other => format!("%{other:02X}"),
        })
        .collect()
}

fn decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;

    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            if let Ok(byte) = u8::from_str_radix(&value[index + 1..index + 3], 16) {
                out.push(byte);
                index += 3;
                continue;
            }
        }
        out.push(bytes[index]);
        index += 1;
    }

    String::from_utf8_lossy(&out).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_with_special_characters_survives_the_url() {
        let config = DbConfig {
            host: "db.local".to_string(),
            port: 5433,
            database: "cockpit".to_string(),
            user: "cockpit".to_string(),
            password: "p@ss:wort/mit#zeichen".to_string(),
        };

        let url = config.url();
        assert!(url.contains("%40"), "At-Zeichen muss kodiert sein: {url}");
        assert!(url.contains("%3A"), "Doppelpunkt muss kodiert sein: {url}");

        let parsed = DbConfig::from_url(&url).unwrap();
        assert_eq!(parsed.password, config.password);
        assert_eq!(parsed.user, config.user);
        assert_eq!(parsed.database, config.database);
        assert_eq!(parsed.port, 5433);
    }

    #[test]
    fn description_never_contains_the_password() {
        let config = DbConfig {
            password: "geheim".to_string(),
            ..DbConfig::default()
        };
        assert!(!config.describe().contains("geheim"));
    }
}
