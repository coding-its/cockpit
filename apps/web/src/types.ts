export type RunStatus = 'planned' | 'in_progress' | 'done' | 'blocked'
export type TaskStatus = 'open' | 'running' | 'done' | 'failed' | 'skipped'
export type Risk = 'low' | 'medium' | 'high'
export type Priority = 'low' | 'normal' | 'high' | 'urgent'
export type SiteStatus = 'healthy' | 'warning' | 'critical' | 'unknown'

export interface CustomerSummary {
  id: string
  name: string
  short_code: string
  ninja_org_id: number | null
  contact_name: string | null
  contact_email: string | null
  phone: string | null
  device_count: number
  site_count: number
  open_runs: number
  month_hours: number
  issues: number
  focus: boolean
}

export interface Customer {
  id: string
  name: string
  short_code: string
  account_id: string | null
  account_code: string | null
  ninja_org_id: number | null
  contact_name: string | null
  contact_email: string | null
  phone: string | null
  notes: string | null
  active: boolean
  focus: boolean
}

export interface Device {
  id: string
  customer_id: string
  ninja_device_id: number | null
  hostname: string
  os: string | null
  device_type: string | null
  last_seen: string | null
  patch_status: string
  antivirus_status: string
  online: boolean
  teamviewer_id: string | null
  patch_pending: number | null
  patch_rejected: number | null
  patch_checked_at: string | null
  antivirus_product: string | null
  antivirus_state: string | null
  antivirus_definitions: string | null
  antivirus_checked_at: string | null
  maintenance_until: string | null
}

export interface DevicePatch {
  patch_id: string
  name: string
  kb_number: string | null
  severity: string | null
  status: string
  patch_type: string | null
  patch_kind: 'os' | 'software'
  observed_at: string
}

export interface Site {
  id: string
  customer_id: string | null
  web_client_id: string | null
  url: string
  label: string | null
  environment: string
  agent_mode: 'rest' | 'wpcli'
  wp_version: string | null
  php_version: string | null
  plugin_updates: number
  core_update: boolean
  status: SiteStatus
  last_checked: string | null
  backup_age_hours: number | null
  notes: string | null
  login_url: string | null
  login_user: string | null
  login_password: string | null
  login_note: string | null
  agent_secret: string | null
  agent_user: string | null
}

export interface Contract {
  id: string
  customer_id: string
  name: string
  interval: string
  included_hours: number
  hourly_rate: number
  flat_fee: number
  billing_day: number
  active: boolean
}

export interface Run {
  id: string
  customer_id: string
  contract_id: string | null
  period: string
  title: string
  scheduled_for: string
  status: RunStatus
  assignee: string | null
  notes: string | null
  created_at: string
  completed_at: string | null
  kind: 'maintenance' | 'ticket'
  priority: Priority
  description: string | null
  channel: string | null
  due_at: string | null
}

export interface RunListItem {
  id: string
  customer_id: string
  customer_name: string
  customer_short_code: string
  period: string
  title: string
  scheduled_for: string
  status: RunStatus
  assignee: string | null
  total_tasks: number
  done_tasks: number
  kind: 'maintenance' | 'ticket'
  priority: Priority
  description: string | null
  due_at: string | null
  channel: string | null
}

export interface Task {
  id: string
  run_id: string
  position: number
  title: string
  description: string | null
  kind: string
  target_type: string | null
  target_id: string | null
  target_label: string | null
  status: TaskStatus
  risk: Risk
  requires_approval: boolean
  approved: boolean
  result_summary: string | null
  started_at: string | null
  finished_at: string | null
}

export interface Job {
  id: string
  task_id: string | null
  kind: string
  status: 'queued' | 'running' | 'succeeded' | 'failed'
  progress: number
  message: string | null
  payload: string | null
  created_at: string
  updated_at: string
}

export interface ExecutionRecord {
  id: string
  task_id: string
  job_id: string | null
  status: string
  request: string | null
  response: string | null
  error: string | null
  duration_ms: number | null
  before_version: string | null
  after_version: string | null
  created_at: string
}

export interface TimeEntry {
  id: string
  customer_id: string | null
  web_client_id: string | null
  owner_name: string
  owner_short_code: string
  site_label: string | null
  device_label: string | null
  customer_name: string
  customer_short_code: string
  run_id: string | null
  task_id: string | null
  task_title: string | null
  user_name: string
  activity: string
  note: string | null
  started_at: string
  ended_at: string | null
  duration_seconds: number
  billable: boolean
  source: 'timer' | 'manual' | 'machine'
  hourly_rate: number
  /** `null`, solange die Zeit noch nicht abgerechnet ist. */
  billed_at: string | null
  /** Auf Cent gerundeter Betrag des Eintrags, vom Server gerechnet. */
  amount: number
}

/** Aufstellung der noch offenen Zeiten eines Kunden. */
export interface Statement {
  kind: 'customer' | 'web'
  owner_id: string
  name: string
  short_code: string
  contact_name: string | null
  contact_email: string | null
  phone: string | null
  contract_name: string | null
  included_hours: number
  hourly_rate: number
  rate_source: 'contract' | 'standard'
  generated_at: string
  generated_by: string
  first_date: string | null
  last_date: string | null
  /** Konto, über das die Aufstellung läuft. */
  account_id: string | null
  account_code: string | null
  /** Datensätze, aus denen die Zeiten stammen. */
  members: StatementMember[]
  /** Mehr als ein Vertrag im Konto – sollte nicht vorkommen. */
  contract_conflict: boolean
  entries: StatementEntry[]
  totals: StatementTotals
}

export interface StatementMember {
  kind: 'customer' | 'web'
  id: string
  name: string
}

export interface StatementEntry {
  id: string
  date: string
  activity: string
  comment: string | null
  context: string | null
  user_name: string
  /** Aus welchem Datensatz die Zeit stammt (bei zusammengefassten Konten). */
  member: string | null
  duration_seconds: number
  billable: boolean
  amount: number
}

export interface StatementTotals {
  entries: number
  billable_seconds: number
  non_billable_seconds: number
  billable_hours: number
  non_billable_hours: number
  time_amount: number
  vat_rate: number
  vat_amount: number
  gross_amount: number
}

export interface ActiveTimer {
  id: string
  user_name: string
  web_client_id: string | null
  device_id: string | null
  device_hostname: string | null
  customer_name: string
  customer_short_code: string
  task_id: string | null
  task_title: string | null
  activity: string
  note: string | null
  started_at: string
  billable: boolean
}

export interface AuditEvent {
  id: string
  actor: string
  action: string
  entity_type: string | null
  entity_id: string | null
  detail: string | null
  created_at: string
}

export interface CustomerDetail {
  customer: Customer
  devices: Device[]
  sites: Site[]
  contracts: Contract[]
  runs: RunListItem[]
  recent_time: TimeEntry[]
}

export interface RunDetail {
  run: Run
  customer: Customer
  tasks: Task[]
  executions: ExecutionRecord[]
  total_time_seconds: number
  machine_time_seconds: number
}

export interface AttentionItem {
  customer_id: string
  customer_name: string
  kind: 'site' | 'device'
  label: string
  detail: string
  severity: 'warning' | 'critical'
}

export interface IntegrationStatus {
  ninjaone_mode: 'mock' | 'live'
  ninjaone_base_url: string
  last_sync: string | null
  wp_agent_configured: boolean
  webhook_slots: string
  user_app_configured: boolean
  user_connected: boolean
  monitoring_checked_at: string | null
  monitoring_patch_devices: number
  monitoring_pending_os: number
  monitoring_pending_software: number
  monitoring_av_warnings: number
}

export interface Dashboard {
  month: string
  open_runs: number
  open_tickets: number
  hidden_customers: number
  billable_hours: number
  billable_amount: number
  customer_count: number
  site_count: number
  critical_sites: number
  pending_patches: number
  next_runs: RunListItem[]
  attention: AttentionItem[]
  recent_activity: AuditEvent[]
  active_timer: ActiveTimer | null
  integration: IntegrationStatus
}

export interface BillingCustomer {
  customer_id: string
  customer_name: string
  short_code: string
  account_id: string | null
  account_code: string | null
  billable_seconds: number
  non_billable_seconds: number
  billable_hours: number
  non_billable_hours: number
  included_hours: number
  hourly_rate: number
  default_rate: boolean
  flat_fee: number
  overage_seconds: number
  overage_hours: number
  time_amount: number
  total_amount: number
}

export interface BillingSummary {
  month: string
  customers: BillingCustomer[]
  total_billable_seconds: number
  total_billable_hours: number
  total_amount: number
}

export interface ExecuteResponse {
  job_id: string
  execution_id: string
}

export interface TimerStopResponse {
  stopped: boolean
  duration_seconds: number
  id: string
  activity: string
  customer_name: string
  customer_short_code: string
}

export interface SyncResult {
  mode: string
  organizations: number
  devices: number
  created_customers: number
  updated_customers: number
  deactivated_demo: number
  last_sync: string
  message: string
}

export interface ManualTimePayload {
  customer_id: string
  task_id?: string | null
  activity: string
  note?: string | null
  date?: string | null
  minutes: number
  billable: boolean
}

export interface CreateRunPayload {
  customer_id: string
  device_id: string
  template: 'script' | 'patch-scan'
  note?: string | null
}

export interface CreateRunResponse {
  run_id: string
  task_id: string
  title: string
  requires_approval: boolean
}

export interface CreateTicketPayload {
  customer_id: string
  device_id?: string | null
  title: string
  description?: string | null
  priority?: Priority
  channel?: string | null
  assignee?: string | null
  due_at?: string | null
}

export interface CreateTicketResponse {
  ticket_id: string
  task_id: string
  title: string
}

export interface CreateSitePayload {
  customer_id: string
  url: string
  label?: string | null
  environment?: 'prod' | 'staging' | 'local'
  agent_user?: string | null
  agent_password?: string | null
  agent_secret?: string | null
  notes?: string | null
}

export interface SiteAgentStatus {
  id: string
  url: string
  label: string | null
  agent_user: string | null
  credentials_complete: boolean
  backup_confirmed: boolean
  backup_confirmed_at: string | null
  backup_reference: string | null
  wp_version: string | null
  php_version: string | null
  plugin_updates: number
  core_update: boolean
  status: string
  last_checked: string | null
}

export interface WebClient {
  id: string
  name: string
  short_code: string
  contact_name: string | null
  contact_email: string | null
  phone: string | null
  website_url: string | null
  notes: string | null
  hourly_rate: number
  active: boolean
  account_id: string | null
  account_code: string | null
}

export interface WebClientSummary {
  id: string
  name: string
  short_code: string
  contact_name: string | null
  contact_email: string | null
  phone: string | null
  website_url: string | null
  hourly_rate: number
  site_count: number
  plugin_updates: number
  month_hours: number
  total_hours: number
  issues: number
  last_activity: string | null
}

export interface WebAction {
  id: string
  site_id: string
  web_client_id: string
  kind: 'health' | 'plugins' | 'core'
  status: 'queued' | 'running' | 'succeeded' | 'failed'
  progress: number
  message: string | null
  detail: string | null
  before_version: string | null
  after_version: string | null
  duration_ms: number | null
  created_at: string
  finished_at: string | null
}

export interface WebClientDetail {
  client: WebClient
  sites: Site[]
  actions: WebAction[]
  time_entries: TimeEntry[]
  month_hours: number
  total_hours: number
}

export interface CreateWebClientPayload {
  name: string
  short_code?: string | null
  contact_name?: string | null
  contact_email?: string | null
  phone?: string | null
  website_url?: string | null
  notes?: string | null
  hourly_rate?: number | null
}

export interface UpdateWebSitePayload {
  url?: string
  label?: string | null
  environment?: 'prod' | 'staging' | 'local'
  agent_user?: string | null
  agent_password?: string | null
  agent_secret?: string | null
  login_url?: string | null
  login_user?: string | null
  login_password?: string | null
  login_note?: string | null
  notes?: string | null
}

export interface CreateWebSitePayload {
  url: string
  label?: string | null
  environment?: 'prod' | 'staging' | 'local'
  agent_user?: string | null
  agent_password?: string | null
  agent_secret?: string | null
  login_url?: string | null
  login_user?: string | null
  login_password?: string | null
  login_note?: string | null
  notes?: string | null
}

export type UserRole = 'admin' | 'technician' | 'viewer'

export interface CurrentUser {
  id: string
  username: string
  display_name: string
  role: UserRole
  role_label: string
  must_change_password: boolean
}

export interface AppUser {
  id: string
  username: string
  display_name: string
  role: UserRole
  active: boolean
  must_change_password: boolean
  created_at: string
  last_login_at: string | null
}

export interface LoginResult {
  user: CurrentUser
  must_change_password: boolean
}

export interface CreateUserPayload {
  username: string
  display_name: string
  password: string
  role: UserRole
}

export interface ReportTotals {
  customers: number
  billable_seconds: number
  non_billable_seconds: number
  billable_hours: number
  non_billable_hours: number
  time_amount: number
  flat_fees: number
  total_amount: number
  vat_rate: number
  vat_amount: number
  gross_amount: number
  runs: number
  actions: number
}

export interface ReportAction {
  kind: string
  label: string
  taken_at: string
  status: string
  message: string | null
  before_version: string | null
  after_version: string | null
}

export interface ReportSite {
  label: string
  url: string
  wp_version: string | null
  plugin_updates: number
  core_update: boolean
  actions: ReportAction[]
}

export interface ReportTask {
  title: string
  target: string | null
  status: string
  result: string | null
}

export interface ReportRun {
  title: string
  taken_at: string
  status: string
  assignee: string | null
  tasks: ReportTask[]
}

export interface ReportEntry {
  date: string
  activity: string
  comment: string | null
  duration_seconds: number
  billable: boolean
  source: string
  context: string | null
  amount: number
  member: string | null
}

export interface ReportBlock {
  kind: 'ninjaone' | 'web'
  customer_id: string
  customer_name: string
  short_code: string
  contact_name: string | null
  contact_email: string | null
  contract_name: string | null
  included_hours: number
  hourly_rate: number
  rate_source: 'contract' | 'standard'
  flat_fee: number
  /** Grundlage aller Beträge: die erfassten Sekunden. */
  billable_seconds: number
  non_billable_seconds: number
  overage_seconds: number
  billable_hours: number
  non_billable_hours: number
  overage_hours: number
  time_amount: number
  total_amount: number
  vat_amount: number
  gross_amount: number
  sites: ReportSite[]
  runs: ReportRun[]
  entries: ReportEntry[]
  account_id: string | null
  account_code: string | null
  /** Namen der Datensätze, die zu diesem Block gehören. */
  members: string[]
  contract_conflict: boolean
}

export interface MonthlyReport {
  month: string
  generated_at: string
  generated_by: string
  blocks: ReportBlock[]
  totals: ReportTotals
}

export interface MonitoringSummary {
  devices_total: number
  devices_with_patch_data: number
  devices_with_pending_patches: number
  devices_with_av_data: number
  devices_with_av_warning: number
  pending_os: number
  pending_software: number
  rejected_os: number
  rejected_software: number
  message: string
}

export interface BillingSettings {
  default_hourly_rate: number
  vat_rate: number
}

/** Abrechnungskonto: fasst mehrere Datensätze zu einer Rechnung zusammen. */
export interface Account {
  id: string
  code: string
  name: string
  active: boolean
  customer_count: number
  web_client_count: number
}

/** TeamViewer-Kundengruppe mit Kennzahlen des Monats. */
export interface TeamViewerGroup {
  group_id: string
  name: string
  customer_id: string | null
  web_client_id: string | null
  assigned_to: string | null
  account_code: string | null
  sessions: number
  open_sessions: number
  seconds: number
  open_seconds: number
  fee: number
  suggested_id: string | null
  suggested_name: string | null
}

/** Einzelne TeamViewer-Verbindung. */
export interface TeamViewerSession {
  id: string
  group_id: string | null
  group_name: string | null
  device_name: string | null
  user_name: string | null
  started_at: string
  ended_at: string | null
  /** Verbindungsdauer laut TeamViewer. */
  duration_seconds: number
  /** Von Hand gesetzt; `null` heißt: Original gilt. */
  duration_override_seconds: number | null
  /** Dauer, die für die Übernahme gilt. */
  effective_seconds: number
  fee: number
  notes: string | null
  note_edit: string | null
  effective_note: string | null
  imported_entry_id: string | null
  ignored: boolean
  unusual: boolean
}

export interface TeamViewerStatus {
  configured: boolean
  sessions: number
  groups: number
  last_sync: string | null
}
