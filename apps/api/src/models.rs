use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct Customer {
    pub id: String,
    pub name: String,
    pub short_code: String,
    pub ninja_org_id: Option<i64>,
    pub contact_name: Option<String>,
    pub contact_email: Option<String>,
    pub phone: Option<String>,
    pub notes: Option<String>,
    pub active: bool,
    pub focus: bool,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct CustomerSummary {
    pub id: String,
    pub name: String,
    pub short_code: String,
    pub ninja_org_id: Option<i64>,
    pub contact_name: Option<String>,
    pub contact_email: Option<String>,
    pub phone: Option<String>,
    pub device_count: i64,
    pub site_count: i64,
    pub open_runs: i64,
    pub month_hours: f64,
    pub issues: i64,
    pub focus: bool,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct Device {
    pub id: String,
    pub customer_id: String,
    pub ninja_device_id: Option<i64>,
    pub hostname: String,
    pub os: Option<String>,
    pub device_type: Option<String>,
    pub last_seen: Option<String>,
    pub patch_status: String,
    pub antivirus_status: String,
    pub online: bool,
    pub teamviewer_id: Option<String>,
    pub patch_pending: Option<i64>,
    pub patch_rejected: Option<i64>,
    pub patch_checked_at: Option<String>,
    pub antivirus_product: Option<String>,
    pub antivirus_state: Option<String>,
    pub antivirus_definitions: Option<String>,
    pub antivirus_checked_at: Option<String>,
    /// Bis wann in NinjaOne ein Wartungsfenster gesetzt ist (ISO, UTC).
    pub maintenance_until: Option<String>,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct Site {
    pub id: String,
    pub customer_id: Option<String>,
    pub web_client_id: Option<String>,
    pub url: String,
    pub label: Option<String>,
    pub environment: String,
    pub agent_mode: String,
    pub wp_version: Option<String>,
    pub php_version: Option<String>,
    pub plugin_updates: i64,
    pub core_update: bool,
    pub status: String,
    pub last_checked: Option<String>,
    pub backup_age_hours: Option<i64>,
    pub notes: Option<String>,
    /// Agent-Benutzer (kein Geheimnis – das Passwort bleibt im Backend).
    pub agent_user: Option<String>,
    pub login_url: Option<String>,
    pub login_user: Option<String>,
    pub login_password: Option<String>,
    pub login_note: Option<String>,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct WebClient {
    pub id: String,
    pub name: String,
    pub short_code: String,
    pub contact_name: Option<String>,
    pub contact_email: Option<String>,
    pub phone: Option<String>,
    pub website_url: Option<String>,
    pub notes: Option<String>,
    pub hourly_rate: f64,
    pub active: bool,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct WebClientSummary {
    pub id: String,
    pub name: String,
    pub short_code: String,
    pub contact_name: Option<String>,
    pub contact_email: Option<String>,
    pub phone: Option<String>,
    pub website_url: Option<String>,
    pub hourly_rate: f64,
    pub site_count: i64,
    pub plugin_updates: i64,
    pub month_hours: f64,
    pub total_hours: f64,
    pub issues: i64,
    pub last_activity: Option<String>,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct WebAction {
    pub id: String,
    pub site_id: String,
    pub web_client_id: String,
    pub kind: String,
    pub status: String,
    pub progress: i64,
    pub message: Option<String>,
    pub detail: Option<String>,
    pub before_version: Option<String>,
    pub after_version: Option<String>,
    pub duration_ms: Option<i64>,
    pub created_at: String,
    pub finished_at: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct WebClientDetail {
    pub client: WebClient,
    pub sites: Vec<Site>,
    pub actions: Vec<WebAction>,
    pub time_entries: Vec<TimeEntryListItem>,
    pub month_hours: f64,
    pub total_hours: f64,
}

#[derive(Debug, Deserialize)]
pub struct CreateWebClientRequest {
    pub name: String,
    pub short_code: Option<String>,
    pub contact_name: Option<String>,
    pub contact_email: Option<String>,
    pub phone: Option<String>,
    pub website_url: Option<String>,
    pub notes: Option<String>,
    pub hourly_rate: Option<f64>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateWebClientRequest {
    pub name: Option<String>,
    pub short_code: Option<String>,
    pub contact_name: Option<String>,
    pub contact_email: Option<String>,
    pub phone: Option<String>,
    pub website_url: Option<String>,
    pub notes: Option<String>,
    pub hourly_rate: Option<f64>,
    pub active: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct CreateWebSiteRequest {
    pub url: String,
    pub label: Option<String>,
    pub environment: Option<String>,
    pub agent_user: Option<String>,
    pub agent_password: Option<String>,
    pub agent_secret: Option<String>,
    pub login_url: Option<String>,
    pub login_user: Option<String>,
    pub login_password: Option<String>,
    pub login_note: Option<String>,
    pub notes: Option<String>,
}

/// Unterscheidet „Feld fehlt" von „Feld ist explizit null".
///
/// Ohne das lässt sich ein einmal gesetzter Wert nie wieder entfernen: `null`
/// wäre nicht von „nicht mitgeschickt" zu unterscheiden.
fn double_option<'de, D>(deserializer: D) -> Result<Option<Option<String>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Some(Option::<String>::deserialize(deserializer)?))
}

#[derive(Debug, Deserialize)]
pub struct UpdateWebSiteRequest {
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    pub label: Option<Option<String>>,
    #[serde(default)]
    pub environment: Option<String>,
    /// Leer gelassen heißt: gespeicherten Agent-Benutzer nicht anfassen.
    #[serde(default, deserialize_with = "double_option")]
    pub agent_user: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub agent_password: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub agent_secret: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub login_url: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub login_user: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub login_password: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub login_note: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub notes: Option<Option<String>>,
}

#[derive(Debug, Deserialize)]
pub struct WebActionRequest {
    pub kind: String,
    pub backup_confirmed: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateCommentRequest {
    pub comment: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct QuickActionRequest {
    pub kind: String,
    /// Nur bei Neustart: harter Neustart statt normalem.
    #[serde(default)]
    pub forced: Option<bool>,
    /// Nur bei Neustart: Begründung, landet im NinjaOne-Protokoll.
    #[serde(default)]
    pub reason: Option<String>,
    /// Nur bei Wartungsmodus: Dauer in Minuten.
    #[serde(default)]
    pub minutes: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct QuickActionResponse {
    pub run_id: String,
    pub task_id: String,
    pub job_id: String,
    pub execution_id: String,
    pub title: String,
}

#[derive(Debug, Deserialize)]
pub struct DeviceTimerRequest {
    pub device_id: String,
    pub activity: Option<String>,
    pub comment: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct WebTimerRequest {
    pub web_client_id: String,
    pub site_id: Option<String>,
    pub activity: Option<String>,
    pub comment: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct WebManualTimeRequest {
    pub web_client_id: String,
    pub site_id: Option<String>,
    pub activity: String,
    pub comment: Option<String>,
    pub date: Option<String>,
    pub minutes: i64,
    pub billable: bool,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct Contract {
    pub id: String,
    pub customer_id: String,
    pub name: String,
    pub interval: String,
    pub included_hours: f64,
    pub hourly_rate: f64,
    pub flat_fee: f64,
    pub billing_day: i64,
    pub active: bool,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct Run {
    pub id: String,
    pub customer_id: String,
    pub contract_id: Option<String>,
    pub period: String,
    pub title: String,
    pub scheduled_for: String,
    pub status: String,
    pub assignee: Option<String>,
    pub notes: Option<String>,
    pub created_at: String,
    pub completed_at: Option<String>,
    pub kind: String,
    pub priority: String,
    pub description: Option<String>,
    pub channel: Option<String>,
    pub due_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct RunListItem {
    pub id: String,
    pub customer_id: String,
    pub customer_name: String,
    pub customer_short_code: String,
    pub period: String,
    pub title: String,
    pub scheduled_for: String,
    pub status: String,
    pub assignee: Option<String>,
    pub total_tasks: i64,
    pub done_tasks: i64,
    pub kind: String,
    pub priority: String,
    pub description: Option<String>,
    pub due_at: Option<String>,
    pub channel: Option<String>,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct Task {
    pub id: String,
    pub run_id: String,
    pub position: i64,
    pub title: String,
    pub description: Option<String>,
    pub kind: String,
    /// Konkrete Aktion, z. B. `patch_apply_software`. Leer bei Aufgaben, die
    /// nur dokumentiert werden.
    pub action: Option<String>,
    /// Zusatzangaben zur Aktion als JSON (Begründung, Dauer, Features).
    pub action_params: Option<String>,
    pub target_type: Option<String>,
    pub target_id: Option<String>,
    pub target_label: Option<String>,
    pub status: String,
    pub risk: String,
    pub requires_approval: bool,
    pub approved: bool,
    pub result_summary: Option<String>,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct Job {
    pub id: String,
    pub task_id: Option<String>,
    pub kind: String,
    pub status: String,
    pub progress: i64,
    pub message: Option<String>,
    pub payload: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct TimeEntryListItem {
    pub id: String,
    pub customer_id: Option<String>,
    pub web_client_id: Option<String>,
    pub owner_name: String,
    pub owner_short_code: String,
    pub site_label: Option<String>,
    pub device_label: Option<String>,
    pub customer_name: String,
    pub customer_short_code: String,
    pub run_id: Option<String>,
    pub task_id: Option<String>,
    pub task_title: Option<String>,
    pub user_name: String,
    pub activity: String,
    pub note: Option<String>,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub duration_seconds: i64,
    pub billable: bool,
    pub source: String,
    pub hourly_rate: f64,
    /// `None` heißt: noch nicht abgerechnet.
    pub billed_at: Option<String>,
    /// Betrag dieses Eintrags, auf Cent gerundet. Wird nach dem Lesen gesetzt,
    /// damit Liste, Aufstellung und Bericht dieselbe Zahl zeigen.
    #[sqlx(default)]
    pub amount: f64,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct ActiveTimer {
    pub id: String,
    /// Wer misst – für die Team-Übersicht der laufenden Timer.
    pub user_name: String,
    pub web_client_id: Option<String>,
    pub device_id: Option<String>,
    pub device_hostname: Option<String>,
    pub customer_name: String,
    pub customer_short_code: String,
    pub task_id: Option<String>,
    pub task_title: Option<String>,
    pub activity: String,
    pub note: Option<String>,
    pub started_at: String,
    pub billable: bool,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct AuditEvent {
    pub id: String,
    pub actor: String,
    pub action: String,
    pub entity_type: Option<String>,
    pub entity_id: Option<String>,
    pub detail: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct ExecutionRecord {
    pub id: String,
    pub task_id: String,
    pub job_id: Option<String>,
    pub status: String,
    pub request: Option<String>,
    pub response: Option<String>,
    pub error: Option<String>,
    pub duration_ms: Option<i64>,
    pub before_version: Option<String>,
    pub after_version: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Serialize)]
pub struct CustomerDetail {
    pub customer: Customer,
    pub devices: Vec<Device>,
    pub sites: Vec<Site>,
    pub contracts: Vec<Contract>,
    pub runs: Vec<RunListItem>,
    pub recent_time: Vec<TimeEntryListItem>,
}

#[derive(Debug, Serialize)]
pub struct RunDetail {
    pub run: Run,
    pub customer: Customer,
    pub tasks: Vec<Task>,
    pub executions: Vec<ExecutionRecord>,
    pub total_time_seconds: i64,
    pub machine_time_seconds: i64,
}

#[derive(Debug, Serialize)]
pub struct AttentionItem {
    pub customer_id: String,
    pub customer_name: String,
    pub kind: String,
    pub label: String,
    pub detail: String,
    pub severity: String,
}

#[derive(Debug, Serialize)]
pub struct IntegrationStatus {
    pub ninjaone_mode: String,
    pub ninjaone_base_url: String,
    pub last_sync: Option<String>,
    pub wp_agent_configured: bool,
    pub webhook_slots: String,
    pub user_app_configured: bool,
    pub user_connected: bool,
    /// Wann Patch- und Antivirusstand zuletzt geholt wurden.
    pub monitoring_checked_at: Option<String>,
    /// Geräte mit Patchdaten (nur die haben eine Aussage).
    pub monitoring_patch_devices: i64,
    /// Offene Betriebssystem-Patches über alle Geräte.
    pub monitoring_pending_os: i64,
    /// Offene Software-Patches über alle Geräte.
    pub monitoring_pending_software: i64,
    /// Geräte, deren Antivirus eine Prüfung braucht.
    pub monitoring_av_warnings: i64,
}

#[derive(Debug, Serialize)]
pub struct Dashboard {
    pub month: String,
    pub open_runs: i64,
    pub open_tickets: i64,
    pub hidden_customers: i64,
    pub billable_hours: f64,
    pub billable_amount: f64,
    pub customer_count: i64,
    pub site_count: i64,
    pub critical_sites: i64,
    pub pending_patches: i64,
    pub next_runs: Vec<RunListItem>,
    pub attention: Vec<AttentionItem>,
    pub recent_activity: Vec<AuditEvent>,
    pub active_timer: Option<ActiveTimer>,
    pub integration: IntegrationStatus,
}

#[derive(Debug, Serialize)]
pub struct BillingCustomer {
    pub customer_id: String,
    pub customer_name: String,
    pub short_code: String,
    /// Grundlage aller Beträge: die erfassten Sekunden.
    pub billable_seconds: i64,
    pub non_billable_seconds: i64,
    pub billable_hours: f64,
    pub non_billable_hours: f64,
    pub included_hours: f64,
    pub hourly_rate: f64,
    /// `true`, wenn kein Vertragssatz vorliegt und der Standard-Satz greift.
    pub default_rate: bool,
    pub flat_fee: f64,
    pub overage_seconds: i64,
    pub overage_hours: f64,
    pub time_amount: f64,
    pub total_amount: f64,
}

#[derive(Debug, Serialize)]
pub struct BillingSummary {
    pub month: String,
    pub customers: Vec<BillingCustomer>,
    pub total_billable_seconds: i64,
    pub total_billable_hours: f64,
    pub total_amount: f64,
}

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
    pub version: &'static str,
    pub database: &'static str,
    pub ninjaone_mode: &'static str,
}

#[derive(Debug, Serialize)]
pub struct SyncResult {
    pub mode: String,
    pub organizations: usize,
    pub devices: usize,
    pub created_customers: usize,
    pub updated_customers: usize,
    pub deactivated_demo: usize,
    pub last_sync: String,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct ExecuteResponse {
    pub job_id: String,
    pub execution_id: String,
}

#[derive(Debug, Serialize)]
pub struct TimerStopResponse {
    pub stopped: bool,
    pub duration_seconds: i64,
    /// ID des Zeiteintrags – der Kommentar-Dialog hängt seinen Text daran.
    pub id: String,
    pub activity: String,
    pub customer_name: String,
    pub customer_short_code: String,
}

#[derive(Debug, Deserialize)]
pub struct RunFilter {
    pub status: Option<String>,
    pub customer_id: Option<String>,
    pub kind: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateTicketRequest {
    pub customer_id: String,
    pub device_id: Option<String>,
    pub title: String,
    pub description: Option<String>,
    pub priority: Option<String>,
    pub channel: Option<String>,
    pub assignee: Option<String>,
    pub due_at: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CreateTicketResponse {
    pub ticket_id: String,
    pub task_id: String,
    pub title: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateRunRequest {
    pub customer_id: String,
    pub device_id: String,
    pub template: String,
    pub note: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CreateRunResponse {
    pub run_id: String,
    pub task_id: String,
    pub title: String,
    pub requires_approval: bool,
}

#[derive(Debug, Deserialize)]
pub struct OAuthCallbackQuery {
    pub code: Option<String>,
    pub state: Option<String>,
    pub error: Option<String>,
    pub error_description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateSiteRequest {
    pub customer_id: String,
    pub url: String,
    pub label: Option<String>,
    pub environment: Option<String>,
    pub agent_user: Option<String>,
    pub agent_password: Option<String>,
    pub agent_secret: Option<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SiteAgentRequest {
    pub agent_user: Option<String>,
    pub agent_password: Option<String>,
    pub agent_secret: Option<String>,
    pub backup_confirmed: Option<bool>,
    pub backup_reference: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SiteAgentStatus {
    pub id: String,
    pub url: String,
    pub label: Option<String>,
    pub agent_user: Option<String>,
    pub credentials_complete: bool,
    pub backup_confirmed: bool,
    pub backup_confirmed_at: Option<String>,
    pub backup_reference: Option<String>,
    pub wp_version: Option<String>,
    pub php_version: Option<String>,
    pub plugin_updates: i64,
    pub core_update: bool,
    pub status: String,
    pub last_checked: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct FocusRequest {
    pub focus: bool,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct DevicePatch {
    pub patch_id: String,
    pub name: String,
    pub kb_number: Option<String>,
    pub severity: Option<String>,
    pub status: String,
    pub patch_type: Option<String>,
    /// `os` oder `software`.
    pub patch_kind: String,
    pub observed_at: String,
}

#[derive(Debug, Deserialize)]
pub struct TeamViewerRequest {
    pub teamviewer_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct TimeEntryFilter {
    pub month: Option<String>,
    pub customer_id: Option<String>,
    pub web_client_id: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct ExportFilter {
    pub month: Option<String>,
    pub customer_id: Option<String>,
    pub web_client_id: Option<String>,
    pub billable_only: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct BillingFilter {
    pub month: Option<String>,
}

/// Auswahl der Zeiten, die als abgerechnet markiert werden sollen.
#[derive(Debug, Deserialize)]
pub struct BilledRequest {
    pub entry_ids: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct BilledResponse {
    /// Wie viele Einträge tatsächlich neu markiert wurden.
    pub marked: u64,
}

#[derive(Debug, Deserialize)]
pub struct TimerStartRequest {
    pub task_id: String,
    pub note: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ManualTimeRequest {
    pub customer_id: String,
    pub task_id: Option<String>,
    pub activity: String,
    pub note: Option<String>,
    pub date: Option<String>,
    pub minutes: i64,
    pub billable: bool,
}
