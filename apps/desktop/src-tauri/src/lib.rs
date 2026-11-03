//! Das Wartungscockpit als Windows-Anwendung.
//!
//! Die Axum-Routen laufen **im selben Prozess** wie das Fenster. Die
//! Oberfläche ruft sie über `invoke("api_call", …)` auf; das ist ein
//! Funktionsaufruf, kein Netzwerk. Es gibt keinen Port, keinen Dienst und
//! nichts, womit sich ein anderer Rechner verbinden könnte – die Datenbank
//! ist die einzige Verbindung nach außen.

use std::sync::Mutex;

use axum::body::Body;
use axum::http::{header, Method, Request, StatusCode};
use serde::Serialize;
use tauri::State;
use tower::ServiceExt;
use wartungscockpit_api::{build_app, start, SharedState};

/// Antwort einer internen Anfrage. Die Oberfläche bekommt Status und Rumpf
/// so, wie sie es vom HTTP-Aufruf im Browser gewohnt ist.
#[derive(Serialize)]
pub struct ApiReply {
    status: u16,
    body: serde_json::Value,
}

pub struct Desktop {
    app: axum::Router,
    /// Das Sitzungscookie. In der Webview gibt es keinen Cookie-Speicher für
    /// interne Aufrufe, deshalb führen wir es selbst.
    session: Mutex<Option<String>>,
}

impl Desktop {
    pub fn new(state: SharedState) -> Self {
        Self {
            app: build_app(state, None),
            session: Mutex::new(None),
        }
    }
}

#[tauri::command]
async fn api_call(
    desktop: State<'_, Desktop>,
    method: String,
    path: String,
    body: Option<String>,
) -> Result<ApiReply, String> {
    let method = Method::from_bytes(method.as_bytes()).map_err(|error| error.to_string())?;

    let mut builder = Request::builder().method(method.clone()).uri(&path);

    if let Some(cookie) = desktop.session.lock().map_err(|e| e.to_string())?.clone() {
        builder = builder.header(header::COOKIE, cookie);
    }

    if body.is_some() {
        builder = builder.header(header::CONTENT_TYPE, "application/json");
    }

    let request = builder
        .body(Body::from(body.unwrap_or_default()))
        .map_err(|error| error.to_string())?;

    let response = desktop
        .app
        .clone()
        .oneshot(request)
        .await
        .map_err(|error| error.to_string())?;

    let status = response.status();

    // Eine neue Sitzung übernehmen, beim Abmelden verwerfen.
    let cookie = response
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .find(|value| value.starts_with("cockpit_session="))
        .map(|value| value.split(';').next().unwrap_or("").to_string());

    if let Some(cookie) = cookie {
        let mut guard = desktop.session.lock().map_err(|e| e.to_string())?;
        if cookie == "cockpit_session=" {
            *guard = None;
        } else {
            *guard = Some(cookie);
        }
    }

    let bytes = axum::body::to_bytes(response.into_body(), 32 * 1024 * 1024)
        .await
        .map_err(|error| error.to_string())?;

    let body = if bytes.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or_else(|_| {
            serde_json::Value::String(String::from_utf8_lossy(&bytes).to_string())
        })
    };

    Ok(ApiReply {
        status: status.as_u16(),
        body,
    })
}

/// Startet die Anwendung neu. Nötig nach Änderungen an der `.env` oder an der
/// Datenbankverbindung, weil beide nur beim Start gelesen werden.
///
/// Bewusst über einen neuen Prozess statt `AppHandle::restart()`: Der Aufruf
/// hat sich hier als wirkungslos erwiesen, das Starten der eigenen Programmdatei
/// ist nachvollziehbar und funktioniert auch in der installierten Fassung.
#[tauri::command]
fn restart_app(app: tauri::AppHandle) {
    match std::env::current_exe() {
        Ok(exe) => {
            if let Err(error) = std::process::Command::new(&exe).spawn() {
                tracing::error!(?error, pfad = %exe.display(), "Neustart fehlgeschlagen");
                return;
            }
            tracing::info!("Neustart angefordert");
            app.exit(0);
        }
        Err(error) => tracing::error!(?error, "Programmdatei nicht gefunden"),
    }
}

/// Meldet, ob die Datenbank erreichbar ist – für die Startanzeige.
#[tauri::command]
async fn backend_status(desktop: State<'_, Desktop>) -> Result<String, String> {
    let request = Request::builder()
        .method(Method::GET)
        .uri("/api/v1/health")
        .body(Body::empty())
        .map_err(|error| error.to_string())?;

    let response = desktop
        .app
        .clone()
        .oneshot(request)
        .await
        .map_err(|error| error.to_string())?;

    if response.status() == StatusCode::OK {
        Ok("ok".to_string())
    } else {
        Ok("fehler".to_string())
    }
}

pub fn run() {
    // Ohne Konsolenfenster landet jede Meldung im Nichts – genau das führt zu
    // „die Anwendung reagiert nicht". Deshalb zusätzlich in eine Datei neben
    // der Konfiguration schreiben, damit ein Fehler nachvollziehbar bleibt.
    let log_path = init_logging();

    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        log = %log_path.as_deref().map(|p| p.display().to_string()).unwrap_or_else(|| "keine Datei".to_string()),
        "Wartungscockpit startet"
    );
    log_environment();

    // `.env` und `data/db.json` werden auch neben der Programmdatei gesucht,
    // damit die Installation aus jedem Startordner funktioniert.
    let env_file = dotenvy::dotenv().ok().or_else(|| {
        let candidates = [
            wartungscockpit_api::DbConfig::path()
                .parent()
                .and_then(|data| data.parent())
                .map(|root| root.join(".env")),
            wartungscockpit_api::DbConfig::app_data_dir().map(|dir| dir.join(".env")),
        ];

        candidates
            .into_iter()
            .flatten()
            .find_map(|candidate| dotenvy::from_path(&candidate).ok().map(|_| candidate))
    });
    if env_file.is_none() {
        tracing::warn!("Keine .env gefunden – NinjaOne läuft im Mock-Modus");
    }

    // Die Datenbank wird vor dem Fenster verbunden. Klappt das nicht, öffnet
    // sich statt der Anwendung ein Einrichtungsfenster: Ohne erreichbare
    // Datenbank wären die Einstellungen im Programm ja selbst nicht erreichbar.
    match tauri::async_runtime::block_on(start()) {
        Ok(state) => {
            let desktop = Desktop::new(state);

            tauri::Builder::default()
                .manage(desktop)
                .invoke_handler(tauri::generate_handler![api_call, backend_status, restart_app])
                .run(tauri::generate_context!())
                .expect("Fenster konnte nicht geöffnet werden");
        }
        Err(error) => {
            let message = startup_message(&error.to_string());
            tracing::error!(meldung = %message, "Datenbank nicht erreichbar – Einrichtung nötig");
            if let Ok(mut slot) = ERROR_TEXT.lock() {
                *slot = message.clone();
            }
            run_setup(message);
        }
    }
}

/// Einrichtungsfenster für den ersten Start und für den Fall, dass die
/// Datenbank nicht erreichbar ist. Enthält nur ein Formular – es braucht
/// selbst keine Datenbank.
fn run_setup(message: String) {
    // Die Seite wird aus dem Speicher über ein eigenes Protokoll geliefert.
    // Sie liegt nicht auf der Platte und kann deshalb auch nicht fehlen.
    setup_page_store(setup_page(&message));

    tauri::Builder::default()
        .register_uri_scheme_protocol("setup", |_ctx, _request| {
            let page = SETUP_PAGE
                .lock()
                .ok()
                .and_then(|slot| slot.clone())
                .unwrap_or_else(|| "<html><body>Seite fehlt</body></html>".to_string());

            tauri::http::Response::builder()
                .header("Content-Type", "text/html; charset=utf-8")
                .body(page.into_bytes())
                .unwrap_or_default()
        })
        .setup(move |app| {
            tauri::WebviewWindowBuilder::new(
                app,
                "setup",
                tauri::WebviewUrl::CustomProtocol("setup://localhost".parse()?),
            )
            .title("Wartungscockpit – Datenbank einrichten")
            .inner_size(620.0, 720.0)
            .resizable(true)
            .center()
            .build()?;

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            save_db_config,
            test_db_config,
            restart_app,
            setup_info
        ])
        .run(tauri::generate_context!())
        .expect("Einrichtungsfenster konnte nicht geöffnet werden");
}

/// Die HTML-Seite des Einrichtungsfensters, einmalig gesetzt.
static SETUP_PAGE: Mutex<Option<String>> = Mutex::new(None);

fn setup_page_store(page: String) {
    if let Ok(mut slot) = SETUP_PAGE.lock() {
        *slot = Some(page);
    }
}

#[derive(Serialize)]
struct SetupInfo {
    host: String,
    port: u16,
    database: String,
    user: String,
    password_set: bool,
    file: String,
    message: String,
}

#[tauri::command]
fn setup_info() -> SetupInfo {
    let (config, _source) = wartungscockpit_api::DbConfig::load();

    setup_info_from(&config, &current_error())
}

fn current_error() -> String {
    ERROR_TEXT.lock().map(|t| t.clone()).unwrap_or_default()
}

static ERROR_TEXT: Mutex<String> = Mutex::new(String::new());

fn setup_info_from(config: &wartungscockpit_api::DbConfig, message: &str) -> SetupInfo {
    SetupInfo {
        host: config.host.clone(),
        port: config.port,
        database: config.database.clone(),
        user: config.user.clone(),
        password_set: !config.password.is_empty(),
        file: wartungscockpit_api::DbConfig::path().display().to_string(),
        message: message.to_string(),
    }
}

#[derive(serde::Deserialize)]
struct SetupRequest {
    host: String,
    port: u16,
    database: String,
    user: String,
    #[serde(default)]
    password: Option<String>,
}

fn build_candidate(payload: &SetupRequest) -> wartungscockpit_api::DbConfig {
    let (current, _source) = wartungscockpit_api::DbConfig::load();

    let password = payload
        .password
        .clone()
        .filter(|value| !value.is_empty())
        .unwrap_or(current.password);

    wartungscockpit_api::DbConfig {
        host: payload.host.trim().to_string(),
        port: payload.port,
        database: payload.database.trim().to_string(),
        user: payload.user.trim().to_string(),
        password,
    }
}

/// Prüft die eingegebene Verbindung, ohne sie zu speichern.
#[tauri::command]
async fn test_db_config(payload: SetupRequest) -> Result<String, String> {
    let candidate = build_candidate(&payload);

    if candidate.host.is_empty() || candidate.database.is_empty() || candidate.user.is_empty() {
        return Err("Server, Datenbank und Benutzer dürfen nicht leer sein.".to_string());
    }

    wartungscockpit_api::test_connection(&candidate.url()).await
}

/// Prüft und speichert die Verbindung – ohne Neustart, damit es prüfbar ist.
async fn store_config(payload: &SetupRequest) -> Result<String, String> {
    let candidate = build_candidate(payload);

    if candidate.host.is_empty() || candidate.database.is_empty() || candidate.user.is_empty() {
        return Err("Server, Datenbank und Benutzer dürfen nicht leer sein.".to_string());
    }

    // Erst verbinden, dann speichern: Eine falsche Adresse darf den nächsten
    // Start nicht unmöglich machen.
    wartungscockpit_api::test_connection(&candidate.url()).await?;

    candidate
        .save()
        .map_err(|error| format!("Datei nicht schreibbar: {error}"))?;

    tracing::info!(ziel = %candidate.describe(), "Datenbankverbindung gespeichert");
    Ok(candidate.describe())
}

/// Speichert die Verbindung und startet die Anwendung neu.
#[tauri::command]
async fn save_db_config(app: tauri::AppHandle, payload: SetupRequest) -> Result<String, String> {
    store_config(&payload).await?;
    restart_after_save(&app);
    Ok("gespeichert".to_string())
}

fn restart_after_save(app: &tauri::AppHandle) {
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::process::Command::new(exe).spawn();
    }
    app.exit(0);
}

/// Die Seite des Einrichtungsfensters. Absichtlich eine einzige Datei ohne
/// Abhängigkeiten: Sie muss auch dann funktionieren, wenn sonst nichts geht.
fn setup_page(message: &str) -> String {
    // Der Text wird als JSON in die Seite gelegt – so sind Anführungszeichen
    // und Zeilenumbrüche automatisch korrekt maskiert.
    let message_json = serde_json::to_string(message).unwrap_or_else(|_| "\"\"".to_string());

    format!(
        r#"<!doctype html>
<html lang="de"><head><meta charset="utf-8">
<title>Wartungscockpit – Datenbank einrichten</title>
<style>
 body{{font-family:system-ui,-apple-system,"Segoe UI",sans-serif;background:#f4f6f8;color:#101828;margin:0;padding:28px}}
 .card{{background:#fff;border:1px solid #e4e7ec;border-radius:10px;padding:22px;max-width:540px;margin:0 auto}}
 h1{{font-size:18px;margin:0 0 6px}} p{{font-size:13px;line-height:1.55;color:#475467;margin:0 0 6px}}
 label{{display:block;font-size:12px;font-weight:600;margin:12px 0 4px}}
 input{{width:100%;box-sizing:border-box;padding:8px 10px;border:1px solid #d0d5dd;border-radius:7px;font-size:13px}}
 .row{{display:flex;gap:10px}} .row>div{{flex:1}}
 button{{margin-top:12px;width:100%;padding:10px;border:0;border-radius:7px;font-size:13px;font-weight:600;cursor:pointer}}
 .primary{{background:#0f766e;color:#fff}} .ghost{{background:#f2f4f7;color:#344054}}
 .msg{{margin-top:12px;padding:10px 12px;border-radius:7px;font-size:12.5px;line-height:1.5;display:none}}
 .ok{{background:#ecfdf3;border:1px solid #abefc6;color:#067647;display:block}}
 .err{{background:#fef3f2;border:1px solid #fecdca;color:#b42318;display:block}}
 .hint{{font-size:11.5px;color:#667085;margin-top:8px}}
 pre{{white-space:pre-wrap;word-break:break-word;font-size:11.5px;color:#475467;background:#f9fafb;border:1px solid #eaecf0;border-radius:7px;padding:10px;margin:12px 0 0;max-height:150px;overflow:auto}}
</style></head><body>
<div class="card">
 <h1>Datenbank einrichten</h1>
 <p>Das Cockpit braucht eine Verbindung zu PostgreSQL. Einmal eintragen, danach startet die Anwendung normal.</p>
 <pre id="why" style="display:none"></pre>
 <div class="row">
   <div><label>Server</label><input id="host" placeholder="192.168.99.88" autofocus></div>
   <div style="max-width:110px"><label>Port</label><input id="port" type="number" value="5432"></div>
 </div>
 <label>Datenbank</label><input id="database" placeholder="wartungscockpit">
 <div class="row">
   <div><label>Benutzer</label><input id="user" placeholder="esc24"></div>
   <div><label>Passwort</label><input id="password" type="password"></div>
 </div>
 <p class="hint" id="pwhint"></p>
 <button class="ghost" onclick="test()">Verbindung testen</button>
 <button class="primary" onclick="save()">Speichern und starten</button>
 <div class="msg" id="msg"></div>
 <p class="hint" id="file"></p>
</div>
<script>
const inv = (cmd, args) => window.__TAURI__.core.invoke(cmd, args);
const START_MESSAGE = {message};

function show(text, ok){{
  const m = document.getElementById('msg');
  m.textContent = text;
  m.className = 'msg ' + (ok ? 'ok' : 'err');
}}

async function load(){{
  const info = await inv('setup_info');
  document.getElementById('host').value = info.host;
  document.getElementById('port').value = info.port;
  document.getElementById('database').value = info.database;
  document.getElementById('user').value = info.user;
  document.getElementById('file').textContent = 'Gespeichert wird in ' + info.file;
  document.getElementById('pwhint').textContent = info.password_set
    ? 'Ein Passwort ist hinterlegt – leer lassen behält es.'
    : '';
  const why = document.getElementById('why');
  if (info.message) {{ why.textContent = info.message; why.style.display = 'block'; }}
  document.getElementById('host').focus();
  document.getElementById('host').select();
}}

function payload(){{
  return {{
    host: document.getElementById('host').value,
    port: parseInt(document.getElementById('port').value || '5432', 10),
    database: document.getElementById('database').value,
    user: document.getElementById('user').value,
    password: document.getElementById('password').value,
  }};
}}

async function test(){{
  show('Prüft …', true);
  try {{ show(await inv('test_db_config', {{ payload: payload() }}), true); }}
  catch (error) {{ show('' + error, false); }}
}}

async function save(){{
  show('Prüft und speichert …', true);
  try {{
    await inv('save_db_config', {{ payload: payload() }});
    show('Gespeichert – die Anwendung startet neu.', true);
  }} catch (error) {{
    show('' + error, false);
  }}
}}

load();
</script></body></html>"#,
        message = message_json
    )
}

/// Bereitet den Fehlertext für den ersten Start auf.
///
/// Ist noch keine Verbindung hinterlegt, wird eine Vorlage geschrieben und der
/// Pfad genannt. Ohne diese Hilfe sieht ein neuer Arbeitsplatz nur „keine
/// Verbindung" und weiß nicht, wohin die Angaben gehören.
fn startup_message(error: &str) -> String {
    let path = wartungscockpit_api::DbConfig::path();

    if path.exists() {
        return format!(
            "{error}\n\nHinterlegte Verbindung: {}\n\nBitte die Angaben prüfen oder hier korrigieren.",
            path.display()
        );
    }

    if let Err(write_error) = wartungscockpit_api::DbConfig::default().save() {
        return format!("{error}\n\nVorlage konnte nicht angelegt werden: {write_error}");
    }

    format!("{error}\n\nEs wurde eine Vorlage angelegt: {}", path.display())
}

/// Pfad der Logdatei – neben Konfiguration und `.env`.
fn log_file() -> Option<std::path::PathBuf> {
    wartungscockpit_api::DbConfig::app_data_dir()
        .map(|dir| dir.join("wartungscockpit.log"))
        .or_else(|| {
            std::env::current_exe()
                .ok()
                .and_then(|exe| exe.parent().map(|dir| dir.join("wartungscockpit.log")))
        })
}

/// Schreibt Logs in eine Datei. Gibt den Pfad zurück, in den geschrieben wird.
///
/// Ohne Konsolenfenster landet jede Meldung sonst im Nichts – und ein Fehler
/// sieht aus wie „die Anwendung reagiert nicht".
fn init_logging() -> Option<std::path::PathBuf> {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));

    let Some(path) = log_file() else {
        tracing_subscriber::fmt().with_env_filter(filter).init();
        return None;
    };

    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    // Die Datei darf nicht unbegrenzt wachsen – ab 2 MB wird sie ersetzt.
    if std::fs::metadata(&path)
        .map(|meta| meta.len() > 2 * 1024 * 1024)
        .unwrap_or(false)
    {
        let _ = std::fs::rename(&path, path.with_extension("log.alt"));
    }

    match std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        Ok(file) => {
            let subscriber = tracing_subscriber::fmt()
                .with_env_filter(filter)
                .with_ansi(false)
                .with_writer(move || file.try_clone().expect("Logdatei gesperrt"));

            // `try_init` statt `init`: Ein zweiter Aufruf im selben Prozess
            // darf nicht in eine Panik laufen.
            if subscriber.try_init().is_err() {
                eprintln!("Logging war bereits gesetzt");
            }
            Some(path)
        }
        Err(error) => {
            eprintln!("Logdatei {} nicht schreibbar: {error}", path.display());
            tracing_subscriber::fmt().with_env_filter(filter).init();
            None
        }
    }
}

/// Notiert, wovon der Start abhängt. Beantwortet beim nächsten „reagiert
/// nicht" die Frage, ob WebView2 vorhanden ist.
fn log_environment() {
    let webview = webview2_version().unwrap_or_else(|| "nicht gefunden".to_string());
    tracing::info!(webview2 = %webview, "Umgebung geprüft");

    let config = wartungscockpit_api::DbConfig::path();
    if config.exists() {
        tracing::info!(datei = %config.display(), "Datenbankkonfiguration vorhanden");
    } else {
        tracing::warn!("Keine Datenbankkonfiguration – Einrichtung nötig");
    }
}

/// Liest die installierte WebView2-Version aus der Registry.
#[cfg(windows)]
fn webview2_version() -> Option<String> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;

    #[link(name = "advapi32")]
    extern "system" {
        fn RegGetValueW(
            hkey: *mut core::ffi::c_void,
            subkey: *const u16,
            value: *const u16,
            flags: u32,
            rtype: *mut u32,
            data: *mut core::ffi::c_void,
            size: *mut u32,
        ) -> i32;
    }

    const HKCU: *mut core::ffi::c_void = 0x8000_0001u32 as usize as *mut core::ffi::c_void;
    const HKLM: *mut core::ffi::c_void = 0x8000_0002u32 as usize as *mut core::ffi::c_void;

    let wide = |text: &str| -> Vec<u16> {
        OsStr::new(text)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    };

    // Ein 64-Bit-Programm liest nur die 64-Bit-Sicht der Registry. WebView2
    // installiert sich aber unter WOW6432Node – ohne diesen Pfad meldet die
    // Prüfung „nicht gefunden", obwohl es vorhanden ist.
    const GUID: &str = r"{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}";
    let paths = [
        (HKCU, format!(r"SOFTWARE\Microsoft\EdgeUpdate\Clients\{GUID}")),
        (HKLM, format!(r"SOFTWARE\Microsoft\EdgeUpdate\Clients\{GUID}")),
        (HKLM, format!(r"SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{GUID}")),
    ];

    let value = wide("pv");

    for (hkey, key) in paths {
        let subkey = wide(&key);
        let mut buffer = [0u16; 64];
        let mut size = (buffer.len() * 2) as u32;

        let status = unsafe {
            RegGetValueW(
                hkey,
                subkey.as_ptr(),
                value.as_ptr(),
                0x0000_0002, // RRF_RT_REG_SZ
                std::ptr::null_mut(),
                buffer.as_mut_ptr() as *mut core::ffi::c_void,
                &mut size,
            )
        };

        if status == 0 {
            let text: String = buffer
                .iter()
                .take_while(|c| **c != 0)
                .map(|c| char::from_u32(*c as u32).unwrap_or('?'))
                .collect();
            if !text.is_empty() {
                return Some(text);
            }
        }
    }

    None
}

#[cfg(not(windows))]
fn webview2_version() -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Das Einrichtungsfenster muss eine falsche Verbindung ablehnen und die
    /// Datei in Ruhe lassen – sonst macht ein Tippfehler den nächsten Start
    /// unmöglich.
    #[tokio::test]
    async fn wrong_credentials_do_not_touch_the_file() {
        let dir = std::env::temp_dir().join("wc-setup-test-falsch");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("DB_CONFIG", dir.join("db.json"));

        let payload = SetupRequest {
            host: "127.0.0.1".to_string(),
            port: 5999,
            database: "gibtsnicht".to_string(),
            user: "niemand".to_string(),
            password: Some("falsch".to_string()),
        };

        let result = store_config(&payload).await;
        assert!(result.is_err(), "falsche Verbindung muss abgelehnt werden");
        assert!(
            !dir.join("db.json").exists(),
            "bei Fehler darf keine Verbindung gespeichert werden"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
