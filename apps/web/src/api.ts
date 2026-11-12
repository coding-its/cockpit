import type {
  ActiveTimer,
  AuditEvent,
  BillingSettings,
  BillingSummary,
  CreateRunPayload,
  CreateRunResponse,
  CreateSitePayload,
  AppUser,
  CreateUserPayload,
  CurrentUser,
  CreateWebClientPayload,
  CreateWebSitePayload,
  WebAction,
  WebClient,
  WebClientDetail,
  WebClientSummary,
  CreateTicketPayload,
  CreateTicketResponse,
  Customer,
  CustomerDetail,
  CustomerSummary,
  Dashboard,
  Device,
  DevicePatch,
  ExecuteResponse,
  IntegrationStatus,
  Job,
  LoginResult,
  ManualTimePayload,
  MonitoringSummary,
  MonthlyReport,
  RunDetail,
  RunListItem,
  Account,
  AccountOverview,
  ActivityKind,
  Site,
  SiteAgentStatus,
  Statement,
  SyncResult,
  TeamViewerGroup,
  TeamViewerSession,
  TeamViewerStatus,
  Task,
  TimeEntry,
  UpdateWebSitePayload,
  TimerStopResponse,
  UserRole,
} from './types'

const BASE = '/api/v1'

/**
 * Läuft die Oberfläche in der Desktop-App? Dort gibt es keinen HTTP-Server:
 * Die Anfragen gehen als Funktionsaufruf in den Rust-Kern (siehe
 * apps/desktop/src-tauri/src/lib.rs). Alles andere bleibt gleich.
 */
const DESKTOP = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window

/** Läuft die Oberfläche in der Desktop-App? */
export const isDesktop = DESKTOP

/**
 * Startet die Desktop-App neu. Änderungen an `.env` und Datenbankverbindung
 * werden beim Start gelesen – im Browser bleibt nur der Hinweis.
 */
export async function restartApp(): Promise<void> {
  if (!DESKTOP) return
  const { invoke } = await import('@tauri-apps/api/core')
  await invoke('restart_app')
}

type DesktopReply = { status: number; body: unknown }

async function desktopRequest<T>(path: string, init?: RequestInit): Promise<T> {
  const { invoke } = await import('@tauri-apps/api/core')

  const reply = await invoke<DesktopReply>('api_call', {
    method: init?.method ?? 'GET',
    path: `${BASE}${path}`,
    body: typeof init?.body === 'string' ? init.body : null,
  })

  if (reply.status === 401) {
    if (!window.location.pathname.startsWith('/login')) {
      const redirect = encodeURIComponent(window.location.pathname + window.location.search)
      window.location.assign(`/login?redirect=${redirect}`)
    }
    throw new Error('Bitte anmelden.')
  }

  if (reply.status < 200 || reply.status >= 300) {
    const payload = reply.body as { error?: string } | null
    throw new Error(payload?.error ?? `Anfrage fehlgeschlagen (${reply.status})`)
  }

  if (reply.status === 204) {
    return undefined as T
  }

  return reply.body as T
}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  if (DESKTOP) return desktopRequest<T>(path, init)

  const response = await fetch(`${BASE}${path}`, {
    ...init,
    headers: {
      'Content-Type': 'application/json',
      ...(init?.headers ?? {}),
    },
  })

  if (response.status === 401) {
    // Sitzung abgelaufen: zurück zur Anmeldung.
    if (!window.location.pathname.startsWith('/login')) {
      const redirect = encodeURIComponent(window.location.pathname + window.location.search)
      window.location.assign(`/login?redirect=${redirect}`)
    }
    throw new Error('Bitte anmelden.')
  }

  if (!response.ok) {
    let message = `Anfrage fehlgeschlagen (${response.status})`
    try {
      const body = (await response.json()) as { error?: string }
      if (body.error) message = body.error
    } catch {
      // Antwort ohne JSON – Standardmeldung behalten.
    }
    throw new Error(message)
  }

  if (response.status === 204) {
    return undefined as T
  }

  return (await response.json()) as T
}

export function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : 'Unbekannter Fehler'
}

export const api = {
  dashboard: () => request<Dashboard>('/dashboard'),
  customers: () => request<CustomerSummary[]>('/customers'),
  customer: (id: string) => request<CustomerDetail>(`/customers/${id}`),
  login: (username: string, password: string) =>
    request<LoginResult>('/auth/login', {
      method: 'POST',
      body: JSON.stringify({ username, password }),
    }),
  logout: () => request<void>('/auth/logout', { method: 'POST' }),
  session: () => request<{ user: CurrentUser | null }>('/auth/me'),
  changeOwnPassword: (currentPassword: string, newPassword: string) =>
    request<CurrentUser>('/auth/password', {
      method: 'POST',
      body: JSON.stringify({ current_password: currentPassword, new_password: newPassword }),
    }),
  users: () => request<AppUser[]>('/users'),
  createUser: (payload: CreateUserPayload) =>
    request<AppUser>('/users', { method: 'POST', body: JSON.stringify(payload) }),
  updateUser: (id: string, payload: { display_name?: string; role?: UserRole; active?: boolean }) =>
    request<AppUser>(`/users/${id}`, { method: 'PATCH', body: JSON.stringify(payload) }),
  resetUserPassword: (id: string, newPassword: string) =>
    request<void>(`/users/${id}/password`, {
      method: 'POST',
      body: JSON.stringify({ new_password: newPassword }),
    }),
  webClients: () => request<WebClientSummary[]>('/web-clients'),
  webClient: (id: string) => request<WebClientDetail>(`/web-clients/${id}`),
  createWebClient: (payload: CreateWebClientPayload) =>
    request<WebClient>('/web-clients', {
      method: 'POST',
      body: JSON.stringify(payload),
    }),
  updateWebClient: (id: string, payload: Partial<CreateWebClientPayload> & { active?: boolean }) =>
    request<WebClient>(`/web-clients/${id}`, {
      method: 'PATCH',
      body: JSON.stringify(payload),
    }),
  deleteWebClient: (id: string) =>
    request<void>(`/web-clients/${id}`, { method: 'DELETE' }),
  createWebSite: (clientId: string, payload: CreateWebSitePayload) =>
    request<Site>(`/web-clients/${clientId}/sites`, {
      method: 'POST',
      body: JSON.stringify(payload),
    }),
  updateWebSite: (siteId: string, payload: UpdateWebSitePayload) =>
    request<Site>(`/web-sites/${siteId}`, {
      method: 'PATCH',
      body: JSON.stringify(payload),
    }),
  deleteWebSite: (siteId: string) =>
    request<void>(`/web-sites/${siteId}`, { method: 'DELETE' }),
  runWebAction: (siteId: string, kind: 'health' | 'plugins' | 'core', backupConfirmed = false) =>
    request<WebAction>(`/web-sites/${siteId}/actions`, {
      method: 'POST',
      body: JSON.stringify({ kind, backup_confirmed: backupConfirmed }),
    }),
  webAction: (id: string) => request<WebAction>(`/web-actions/${id}`),
  startWebTimer: (payload: {
    web_client_id: string
    site_id?: string | null
    activity?: string
    comment?: string
  }) =>
    request<ActiveTimer | null>('/web-timer/start', {
      method: 'POST',
      body: JSON.stringify(payload),
    }),
  createWebTimeEntry: (payload: {
    web_client_id: string
    site_id?: string | null
    activity: string
    comment?: string | null
    date?: string | null
    minutes: number
    billable: boolean
  }) =>
    request<TimeEntry>('/web-time-entries', {
      method: 'POST',
      body: JSON.stringify(payload),
    }),
  setTimeComment: (id: string, comment: string) =>
    request<TimeEntry>(`/time-entries/${id}/comment`, {
      method: 'PATCH',
      body: JSON.stringify({ comment }),
    }),
  createSite: (payload: CreateSitePayload) =>
    request<SiteAgentStatus>('/sites', {
      method: 'POST',
      body: JSON.stringify(payload),
    }),
  siteAgent: (id: string) => request<SiteAgentStatus>(`/sites/${id}/agent`),
  setSiteAgent: (
    id: string,
    payload: {
      agent_user?: string
      agent_password?: string
      agent_secret?: string
      backup_confirmed?: boolean
      backup_reference?: string
    },
  ) =>
    request<SiteAgentStatus>(`/sites/${id}/agent`, {
      method: 'PATCH',
      body: JSON.stringify(payload),
    }),
  setCustomerFocus: (id: string, focus: boolean) =>
    request<CustomerSummary>(`/customers/${id}/focus`, {
      method: 'PATCH',
      body: JSON.stringify({ focus }),
    }),
  runs: (status?: string, kind?: string) => {
    const params = new URLSearchParams()
    if (status) params.set('status', status)
    if (kind) params.set('kind', kind)
    const suffix = params.toString()
    return request<RunListItem[]>(`/runs${suffix ? `?${suffix}` : ''}`)
  },
  tickets: () => request<RunListItem[]>('/tickets'),
  createTicket: (payload: CreateTicketPayload) =>
    request<CreateTicketResponse>('/tickets', {
      method: 'POST',
      body: JSON.stringify(payload),
    }),
  run: (id: string) => request<RunDetail>(`/runs/${id}`),
  createRun: (payload: CreateRunPayload) =>
    request<CreateRunResponse>('/runs', {
      method: 'POST',
      body: JSON.stringify(payload),
    }),
  task: (id: string) => request<Task>(`/tasks/${id}`),
  approveTask: (id: string) => request<Task>(`/tasks/${id}/approve`, { method: 'POST' }),
  executeTask: (id: string) =>
    request<ExecuteResponse>(`/tasks/${id}/execute`, { method: 'POST' }),
  job: (id: string) => request<Job>(`/jobs/${id}`),
  timer: () => request<ActiveTimer | null>('/timer/active'),
  runningTimers: () => request<ActiveTimer[]>('/timer/running'),
  startTimer: (taskId: string, note?: string) =>
    request<ActiveTimer | null>('/timer/start', {
      method: 'POST',
      body: JSON.stringify({ task_id: taskId, note: note ?? null }),
    }),
  devicePatches: (deviceId: string) => request<DevicePatch[]>(`/devices/${deviceId}/patches`),
  setDeviceTeamviewer: (deviceId: string, teamviewerId: string | null) =>
    request<Device>(`/devices/${deviceId}/teamviewer`, {
      method: 'PATCH',
      body: JSON.stringify({ teamviewer_id: teamviewerId }),
    }),
  quickDeviceAction: (
    deviceId: string,
    kind:
      | 'script'
      | 'patch-scan'
      | 'software-scan'
      | 'patch-apply'
      | 'software-apply'
      | 'reboot'
      | 'maintenance-start'
      | 'maintenance-stop',
    options?: { forced?: boolean; reason?: string; minutes?: number },
  ) =>
    request<{ run_id: string; task_id: string; job_id: string; execution_id: string; title: string }>(
      `/devices/${deviceId}/quick-action`,
      { method: 'POST', body: JSON.stringify({ kind, ...options }) },
    ),
  startDeviceTimer: (payload: { device_id: string; activity?: string; comment?: string }) =>
    request<ActiveTimer | null>('/device-timer/start', {
      method: 'POST',
      body: JSON.stringify(payload),
    }),
  stopTimer: () => request<TimerStopResponse | null>('/timer/stop', { method: 'POST' }),
  timeEntries: (month?: string, owner?: { customerId?: string; webClientId?: string }) => {
    const params = new URLSearchParams()
    if (month) params.set('month', month)
    if (owner?.customerId) params.set('customer_id', owner.customerId)
    if (owner?.webClientId) params.set('web_client_id', owner.webClientId)
    const suffix = params.toString()
    return request<TimeEntry[]>(`/time-entries${suffix ? `?${suffix}` : ''}`)
  },
  createTimeEntry: (payload: ManualTimePayload) =>
    request<TimeEntry>('/time-entries', {
      method: 'POST',
      body: JSON.stringify(payload),
    }),
  deleteTimeEntry: (id: string) =>
    request<void>(`/time-entries/${id}`, { method: 'DELETE' }),
  monthlyReport: (month: string, includeIdle = false) =>
    request<MonthlyReport>(
      `/reports/monthly?month=${encodeURIComponent(month)}&include_idle=${includeIdle}`,
    ),
  billing: (month: string) =>
    request<BillingSummary>(`/billing/summary?month=${encodeURIComponent(month)}`),
  statement: (customerId: string) =>
    request<Statement>(`/customers/${customerId}/statement`),
  markStatementBilled: (customerId: string, entryIds: string[]) =>
    request<{ marked: number }>(`/customers/${customerId}/statement/billed`, {
      method: 'POST',
      body: JSON.stringify({ entry_ids: entryIds }),
    }),
  accounts: () => request<Account[]>('/accounts'),
  accountsOverview: () => request<AccountOverview>('/accounts/overview'),
  activities: (includeInactive = false) =>
    request<ActivityKind[]>(`/activities?include_inactive=${includeInactive}`),
  createActivity: (name: string) =>
    request<ActivityKind>('/activities', { method: 'POST', body: JSON.stringify({ name }) }),
  updateActivity: (id: string, payload: { name?: string; active?: boolean }) =>
    request<ActivityKind>(`/activities/${id}`, {
      method: 'PATCH',
      body: JSON.stringify(payload),
    }),
  assignGroupToAccount: (groupId: string, accountId: string | null) =>
    request<{ ok: boolean }>(`/teamviewer/groups/${groupId}/account`, {
      method: 'PATCH',
      body: JSON.stringify({ account_id: accountId }),
    }),
  teamviewerUndoImport: (sessionId: string) =>
    request<{ ok: boolean }>(`/teamviewer/sessions/${sessionId}/import`, { method: 'DELETE' }),
  teamviewerStatus: () => request<TeamViewerStatus>('/teamviewer/status'),
  teamviewerSettings: () =>
    request<{ configured: boolean; source: string | null }>('/settings/teamviewer'),
  updateTeamViewerSettings: (payload: { token?: string; clear?: boolean; test_only?: boolean }) =>
    request<{ ok: boolean; configured: boolean; report?: string; file?: string }>(
      '/settings/teamviewer',
      { method: 'PUT', body: JSON.stringify(payload) },
    ),
  testTeamViewerSettings: (token: string) =>
    request<{ ok: boolean; report: string }>('/settings/teamviewer/test', {
      method: 'POST',
      body: JSON.stringify({ token }),
    }),
  teamviewerGroups: (month: string) =>
    request<TeamViewerGroup[]>(`/teamviewer?month=${encodeURIComponent(month)}`),
  teamviewerSessions: (month: string, groupId?: string) => {
    const params = new URLSearchParams({ month })
    if (groupId) params.set('group_id', groupId)
    return request<TeamViewerSession[]>(`/teamviewer/sessions?${params.toString()}`)
  },
  teamviewerSync: (month: string) =>
    request<{ groups: number; sessions: number; message: string }>(
      `/teamviewer/sync?month=${encodeURIComponent(month)}`,
      { method: 'POST' },
    ),
  teamviewerAssign: (
    groupId: string,
    payload: { customer_id?: string | null; web_client_id?: string | null; ignored?: boolean },
  ) =>
    request<{ ok: boolean }>(`/teamviewer/groups/${groupId}`, {
      method: 'PATCH',
      body: JSON.stringify(payload),
    }),
  teamviewerEditSession: (
    sessionId: string,
    payload: {
      note?: string
      duration_seconds?: number
      ignored?: boolean
      activity?: string
    },
  ) =>
    request<{ ok: boolean }>(`/teamviewer/sessions/${sessionId}`, {
      method: 'PATCH',
      body: JSON.stringify(payload),
    }),
  teamviewerImport: (groupId: string, month: string, billable = true) =>
    request<{ created: number; skipped: number; message: string }>(
      `/teamviewer/groups/${groupId}/import`,
      { method: 'POST', body: JSON.stringify({ month, billable }) },
    ),
  createAccount: (payload: { code: string; name: string }) =>
    request<Account>('/accounts', { method: 'POST', body: JSON.stringify(payload) }),
  updateAccount: (
    id: string,
    payload: { code?: string; name?: string; active?: boolean },
  ) =>
    request<Account>(`/accounts/${id}`, {
      method: 'PATCH',
      body: JSON.stringify(payload),
    }),
  deleteAccount: (id: string) =>
    request<void>(`/accounts/${id}`, { method: 'DELETE' }),
  setCustomerAccount: (customerId: string, accountId: string | null) =>
    request<Customer>(`/customers/${customerId}/account`, {
      method: 'PATCH',
      body: JSON.stringify({ account_id: accountId }),
    }),
  setWebClientAccount: (webClientId: string, accountId: string | null) =>
    request<WebClient>(`/web-clients/${webClientId}/account`, {
      method: 'PATCH',
      body: JSON.stringify({ account_id: accountId }),
    }),
  accountStatement: (accountId: string) =>
    request<Statement>(`/accounts/${accountId}/statement`),
  markAccountStatementBilled: (accountId: string, entryIds: string[]) =>
    request<{ marked: number }>(`/accounts/${accountId}/statement/billed`, {
      method: 'POST',
      body: JSON.stringify({ entry_ids: entryIds }),
    }),
  webStatement: (webClientId: string) =>
    request<Statement>(`/web-clients/${webClientId}/statement`),
  markWebStatementBilled: (webClientId: string, entryIds: string[]) =>
    request<{ marked: number }>(`/web-clients/${webClientId}/statement/billed`, {
      method: 'POST',
      body: JSON.stringify({ entry_ids: entryIds }),
    }),
  exportCsv: (month: string) => {
    window.open(`${BASE}/export/time-entries.csv?month=${encodeURIComponent(month)}`, '_blank')
  },
  envSettings: () =>
    request<{
      mock: boolean
      base_url: string
      client_id: string
      client_secret_set: boolean
      scope: string
      user_client_id: string
      user_client_secret_set: boolean
      redirect_uri: string
      file: string
    }>('/settings/env'),
  updateEnvSettings: (payload: {
    mock: boolean
    base_url: string
    client_id: string
    client_secret?: string
    scope: string
    user_client_id: string
    user_client_secret?: string
    redirect_uri: string
  }) =>
    request<{ ok: boolean; report?: string; restart_required?: boolean }>('/settings/env', {
      method: 'PUT',
      body: JSON.stringify(payload),
    }),
  testEnvSettings: (payload: {
    mock: boolean
    base_url: string
    client_id: string
    client_secret?: string
    scope: string
    user_client_id: string
    user_client_secret?: string
    redirect_uri: string
  }) =>
    request<{ ok: boolean; report?: string }>('/settings/env/test', {
      method: 'POST',
      body: JSON.stringify(payload),
    }),
  databaseSettings: () =>
    request<{
      host: string
      port: number
      database: string
      user: string
      password_set: boolean
      file: string
    }>('/settings/database'),
  updateDatabaseSettings: (payload: {
    host: string
    port: number
    database: string
    user: string
    password?: string
    test_only?: boolean
  }) =>
    request<{ ok: boolean; report?: string; restart_required?: boolean }>(
      '/settings/database',
      { method: 'PUT', body: JSON.stringify(payload) },
    ),
  billingSettings: () => request<BillingSettings>('/settings/billing'),
  updateBillingSettings: (payload: Partial<BillingSettings>) =>
    request<BillingSettings>('/settings/billing', {
      method: 'PUT',
      body: JSON.stringify(payload),
    }),
  sync: () => request<SyncResult>('/sync/ninjaone', { method: 'POST' }),
  syncMonitoring: () => request<MonitoringSummary>('/sync/monitoring', { method: 'POST' }),
  integrations: () => request<IntegrationStatus>('/integrations'),
  ninjaoneAuthStart: () => request<{ url: string }>('/auth/ninjaone/start'),
  audit: () => request<AuditEvent[]>('/audit'),
}
