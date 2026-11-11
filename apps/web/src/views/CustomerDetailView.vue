<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
import { RouterLink, useRoute, useRouter } from 'vue-router'
import {
  ArrowDownToLine,
  ArrowLeft,
  BellOff,
  BellRing,
  Building2,
  CheckCircle2,
  ChevronRight,
  Clock3,
  Euro,
  Eye,
  Globe2,
  HardDrive,
  Mail,
  MonitorPlay,
  MonitorSmartphone,
  PackageSearch,
  Phone,
  Play,
  Plus,
  Power,
  Printer,
  Server,
  ShieldAlert,
  ShieldCheck,
  Square,
  Terminal,
  Wifi,
  WifiOff,
  Wrench,
  X,
} from 'lucide-vue-next'
import { api, errorMessage } from '../api'
import type { Account, BillingSummary, CustomerDetail, DevicePatch } from '../types'
import {
  currency,
  currentMonth,
  formatDate,
  formatDateTime,
  formatDuration,
  formatHours,
  formatTimer,
  initials,
  ninjaDeviceUrl,
  monthLabel,
  sourceLabel,
  teamViewerUrl,
} from '../format'
import Modal from '../components/Modal.vue'
import ProgressBar from '../components/ProgressBar.vue'
import RunCard from '../components/RunCard.vue'
import StatusBadge from '../components/StatusBadge.vue'
import { useAppStore } from '../stores/app'

const route = useRoute()
const router = useRouter()
const store = useAppStore()
const detail = ref<CustomerDetail | null>(null)
const accounts = ref<Account[]>([])
const accountBusy = ref(false)
const accountError = ref('')
const billing = ref<BillingSummary | null>(null)
const loading = ref(true)
const error = ref('')
const month = currentMonth()
const selectedDeviceId = ref('')
const shownDeviceId = ref('')
const quickTestBusy = ref<DeviceActionKind | null>(null)
const integrationBaseUrl = ref('https://eu.ninjarmm.com')

type DeviceActionKind =
  | 'script'
  | 'patch-scan'
  | 'software-scan'
  | 'patch-apply'
  | 'software-apply'
  | 'reboot'
  | 'maintenance-start'
  | 'maintenance-stop'

/// Aktionen, die ohne Rückfrage laufen: sie ändern nichts am Gerät.
const QUIET_ACTIONS: DeviceActionKind[] = ['patch-scan', 'software-scan', 'script']

const ACTION_TITLES: Record<DeviceActionKind, string> = {
  script: 'Script-Test',
  'patch-scan': 'OS-Scan',
  'software-scan': 'Software-Scan',
  'patch-apply': 'OS-Updates',
  'software-apply': 'Software-Updates',
  reboot: 'Neustart',
  'maintenance-start': 'Wartungsmodus',
  'maintenance-stop': 'Wartungsmodus beenden',
}

const pendingAction = ref<DeviceActionKind | null>(null)
const rebootForced = ref(false)
const rebootReason = ref('Wartung über Wartungscockpit')
const maintenanceMinutes = ref(60)

/// Bis wann der Wartungsmodus läuft – aus unserer eigenen Notiz, weil
/// NinjaOne dafür keinen Lese-Endpunkt hat.
const maintenanceActive = computed(() => {
  const until = shownDevice.value?.maintenance_until
  if (!until) return false
  return new Date(until).getTime() > Date.now()
})

const maintenanceUntilLabel = computed(() => {
  const until = shownDevice.value?.maintenance_until
  if (!until) return ''
  return formatDateTime(until)
})
const inlineAction = ref<{
  title: string
  status: string
  progress: number
  message: string | null
} | null>(null)
const timerSection = ref<HTMLElement | null>(null)
let inlinePoller: number | undefined
const patches = ref<DevicePatch[]>([])
const patchesLoading = ref(false)
const patchesOpen = ref(false)
const patchFilter = ref<'os' | 'software'>('os')
const teamviewerEditing = ref(false)
const teamviewerDraft = ref('')
const teamviewerBusy = ref(false)
const deviceActivity = ref('Gerätewartung')
const deviceComment = ref('')
const now = ref(Date.now())
let clock: number | undefined

watch(shownDeviceId, async (id) => {
  teamviewerEditing.value = false
  patches.value = []
  patchesOpen.value = false
  patchFilter.value = 'os'
  if (!id) return

  patchesLoading.value = true
  try {
    patches.value = await api.devicePatches(id)
    // Mit der Art starten, zu der es auch Einträge gibt – sonst steht der
    // Nutzer vor einer leeren Liste und muss erst den Reiter wechseln.
    const hasOs = patches.value.some((patch) => patch.patch_kind === 'os')
    patchFilter.value = hasOs ? 'os' : 'software'
  } catch {
    // Ohne Patchliste funktioniert das Panel weiter; die Kacheln zeigen den Stand.
  } finally {
    patchesLoading.value = false
  }
})

/** Patches der gewählten Art – Betriebssystem und Programme getrennt. */
const visiblePatches = computed(() =>
  patches.value.filter((patch) => patch.patch_kind === patchFilter.value),
)

const osPatchCount = computed(
  () => patches.value.filter((patch) => patch.patch_kind === 'os').length,
)
const softwarePatchCount = computed(
  () => patches.value.filter((patch) => patch.patch_kind === 'software').length,
)

/** Kurzfassung für die Patch-Kachel: „3 offen · 1 abgelehnt". */
function patchSummary(device: CustomerDetail['devices'][number]): string {
  const pending = device.patch_pending ?? 0
  const rejected = device.patch_rejected ?? 0

  if (device.patch_status === 'unknown') return 'keine Daten'
  if (pending === 0 && rejected === 0) return 'nichts offen'

  const parts: string[] = []
  if (pending > 0) parts.push(`${pending} offen`)
  if (rejected > 0) parts.push(`${rejected} abgelehnt`)
  return parts.join(' · ')
}

/** Zeile unter der Antivirus-Kachel: Produkt und Definitionsstand. */
function antivirusDetail(device: CustomerDetail['devices'][number]): string {
  if (!device.antivirus_product) return 'keine Daten'

  const parts = [device.antivirus_product]
  if (device.antivirus_state && device.antivirus_state !== 'ON') {
    parts.push(device.antivirus_state === 'Expired' ? 'abgelaufen' : 'aus')
  }
  if (device.antivirus_definitions) {
    parts.push(
      device.antivirus_definitions === 'Out-of-Date' ? 'Signaturen veraltet' : 'Signaturen aktuell',
    )
  }
  return parts.join(' · ')
}

/** KB-Nummer und Schweregrad; `NONE`/`UNASSIGNED` sind bei NinjaOne Platzhalter. */
function patchMeta(patch: DevicePatch): string {
  const severity = patch.severity?.toUpperCase()
  const meaningful = severity && !['NONE', 'UNASSIGNED', ''].includes(severity)
  return [patch.kb_number, meaningful ? severity : null].filter(Boolean).join(' · ')
}

function patchStatusLabel(status: string): string {
  return (
    {
      APPROVED: 'offen',
      DELAYED: 'zurückgestellt',
      MANUAL: 'manuell',
      REJECTED: 'abgelehnt',
      INSTALLED: 'installiert',
    }[status] ?? status
  )
}

const shownDevice = computed(
  () => detail.value?.devices.find((device) => device.id === shownDeviceId.value) ?? null,
)

const deviceTimerRunning = computed(
  () => !!store.activeTimer && store.activeTimer.device_id === shownDeviceId.value,
)

const deviceElapsed = computed(() => {
  const timer = store.activeTimer
  if (!timer || timer.device_id !== shownDeviceId.value) return 0
  const started = new Date(timer.started_at).getTime()
  if (Number.isNaN(started)) return 0
  return Math.max(0, Math.floor((now.value - started) / 1000))
})

/** Geräte, die in der Kundenübersicht als „Handlungsbedarf" zählen. */
function hasIssue(device: CustomerDetail['devices'][number]): boolean {
  return device.patch_status === 'pending' || device.antivirus_status === 'warning'
}

const issueDevices = computed(() => (detail.value?.devices ?? []).filter(hasIssue))

/** Warum ein Gerät markiert ist – in einem Satz, für die Liste. */
function issueReason(device: CustomerDetail['devices'][number]): string {
  const parts: string[] = []

  if (device.antivirus_status === 'warning') {
    const stale = device.antivirus_definitions === 'Out-of-Date'
    const product = device.antivirus_product ?? 'Antivirus'
    parts.push(`${product} ${stale ? '– Signaturen veraltet' : 'inaktiv'}`)
  }

  if (device.patch_status === 'pending') {
    const pending = device.patch_pending ?? 0
    parts.push(`${pending} ${pending === 1 ? 'Patch' : 'Patches'} offen`)
  }

  return parts.join(' · ')
}

function showDevice(deviceId?: string) {
  if (deviceId) selectedDeviceId.value = deviceId
  shownDeviceId.value = selectedDeviceId.value
  inlineAction.value = null
  if (inlinePoller) {
    window.clearInterval(inlinePoller)
    inlinePoller = undefined
  }
  autoStartDeviceTimer()
}

/**
 * Startet den Timer beim Öffnen eines Geräts – ohne einen laufenden Timer zu
 * beenden. Wer schon woanders misst, behält seine Zeit; sonst würde die
 * automatische Erfassung stillschweigend Abrechnungsdaten abschneiden.
 */
async function autoStartDeviceTimer() {
  if (!shownDeviceId.value) return

  const running = store.activeTimer
  if (running) {
    // Läuft schon auf genau diesem Gerät: nichts zu tun.
    if (running.device_id === shownDeviceId.value) return

    store.toast(
      `Timer läuft noch für ${running.customer_short_code ?? running.customer_name ?? 'einen anderen Vorgang'} – bitte zuerst stoppen.`,
      'info',
    )
    return
  }

  try {
    await api.startDeviceTimer({
      device_id: shownDeviceId.value,
      activity: deviceActivity.value,
      comment: '',
    })
    await store.loadTimer()
    store.toast('Timer gestartet – Zeit läuft auf diesem Gerät.', 'success')
  } catch (caught) {
    // Ohne Timer lässt sich weiterarbeiten; das Panel zeigt den Fehler nicht an.
    store.toast(errorMessage(caught), 'error')
  }
}

/**
 * Schließt das Geräte-Panel. Läuft der Timer auf genau diesem Gerät, wird er
 * dabei gestoppt – das öffnet den Kommentar-Dialog. Wer das Panel nur kurz
 * zum Nachsehen schließt, verliert so keine Zeit im Hintergrund.
 */
async function closeDevice() {
  const wasRunning = deviceTimerRunning.value
  shownDeviceId.value = ''

  if (!wasRunning) return

  try {
    await store.stopTimer()
  } catch (caught) {
    store.toast(errorMessage(caught), 'error')
  }
}

async function startDeviceTimer() {
  if (!shownDeviceId.value) return
  try {
    await api.startDeviceTimer({
      device_id: shownDeviceId.value,
      activity: deviceActivity.value,
      comment: deviceComment.value,
    })
    await store.loadTimer()
    deviceComment.value = ''
    store.toast('Timer läuft auf diesem Gerät.', 'success')
  } catch (caught) {
    store.toast(errorMessage(caught), 'error')
  }
}

async function stopTimer() {
  try {
    await store.stopTimer()
    await load()
  } catch (caught) {
    store.toast(errorMessage(caught), 'error')
  }
}

function remoteUrl(deviceId: number | null): string | null {
  return ninjaDeviceUrl(integrationBaseUrl.value, deviceId)
}

/** Öffnet TeamViewer direkt mit dieser Geräte-ID – ein Klick, kein Suchen. */
function teamviewerLink(teamviewerId: string | null): string | null {
  return teamViewerUrl(teamviewerId)
}

function editTeamviewer() {
  teamviewerDraft.value = shownDevice.value?.teamviewer_id ?? ''
  teamviewerEditing.value = true
}

async function saveTeamviewer() {
  const device = shownDevice.value
  if (!device || teamviewerBusy.value) return

  teamviewerBusy.value = true
  try {
    const updated = await api.setDeviceTeamviewer(device.id, teamviewerDraft.value.trim() || null)
    const list = detail.value?.devices
    const index = list?.findIndex((entry) => entry.id === updated.id) ?? -1
    if (list && index >= 0) list[index] = updated
    teamviewerEditing.value = false
    store.toast(
      updated.teamviewer_id
        ? `TeamViewer-ID ${updated.teamviewer_id} gespeichert.`
        : 'TeamViewer-Verknüpfung entfernt.',
      'success',
    )
  } catch (caught) {
    store.toast(errorMessage(caught), 'error')
  } finally {
    teamviewerBusy.value = false
  }
}


const customerId = computed(() => route.params.id as string)
const billingRow = computed(
  () => billing.value?.customers.find((row) => row.customer_id === customerId.value) ?? null,
)
const contract = computed(() => detail.value?.contracts[0] ?? null)
const includedProgress = computed(() => {
  if (!contract.value || !billingRow.value || contract.value.included_hours === 0) return 0
  return Math.min(100, (billingRow.value.billable_hours / contract.value.included_hours) * 100)
})
const totalRecentTime = computed(
  () => detail.value?.recent_time.reduce((sum, entry) => sum + entry.duration_seconds, 0) ?? 0,
)

async function load() {
  loading.value = true
  error.value = ''
  try {
    const [customer, billingSummary, integration, accountList] = await Promise.all([
      api.customer(customerId.value),
      api.billing(month),
      api.integrations().catch(() => null),
      api.accounts().catch(() => [] as Account[]),
    ])
    accounts.value = accountList
    detail.value = customer
    billing.value = billingSummary
    if (integration?.ninjaone_base_url) {
      integrationBaseUrl.value = integration.ninjaone_base_url
    }
    const preferred =
      customer.devices.find((device) => device.online && device.device_type === 'server') ??
      customer.devices.find((device) => device.device_type === 'server') ??
      customer.devices[0]
    if (!customer.devices.some((device) => device.id === selectedDeviceId.value)) {
      selectedDeviceId.value = preferred?.id ?? ''
    }
  } catch (caught) {
    error.value = errorMessage(caught)
  } finally {
    loading.value = false
  }
}

/// Scan und Test laufen sofort. Alles, was am Gerät etwas verändert,
/// geht über eine Rückfrage – ein Fehlklick soll nicht installieren.
function askAction(kind: DeviceActionKind) {
  if (!shownDeviceId.value) return
  if (QUIET_ACTIONS.includes(kind)) {
    runDeviceAction(kind)
    return
  }

  rebootForced.value = false
  pendingAction.value = kind
}

function confirmAction() {
  const kind = pendingAction.value
  if (!kind) return
  pendingAction.value = null

  if (kind === 'reboot') {
    runDeviceAction(kind, { forced: rebootForced.value, reason: rebootReason.value.trim() })
  } else if (kind === 'maintenance-start') {
    runDeviceAction(kind, { minutes: maintenanceMinutes.value })
  } else {
    runDeviceAction(kind)
  }
}

async function runDeviceAction(
  kind: DeviceActionKind,
  options?: { forced?: boolean; reason?: string; minutes?: number },
) {
  if (!shownDeviceId.value) return
  const title = ACTION_TITLES[kind]

  quickTestBusy.value = kind
  inlineAction.value = {
    title,
    status: 'queued',
    progress: 0,
    message: 'Wird vorbereitet …',
  }

  try {
    const result = await api.quickDeviceAction(shownDeviceId.value, kind, options)
    pollInlineAction(result.job_id, result.task_id)
  } catch (caught) {
    quickTestBusy.value = null
    inlineAction.value = {
      title,
      status: 'failed',
      progress: 100,
      message: errorMessage(caught),
    }
  }
}

/// Verfolgt die Aktion direkt im Panel, ohne Seitenwechsel.
/// Der Abschluss kommt bei NinjaOne oft später. Wir schauen drei Minuten
/// nach, ob der Nachweis nachgetragen wurde, und aktualisieren die Anzeige.
async function followUpProof(taskId: string) {
  for (let attempt = 0; attempt < 18; attempt++) {
    await new Promise((resolve) => window.setTimeout(resolve, 10_000))
    try {
      const task = await api.task(taskId)

      // Neustart und Wartungsmodus sind mit der Annahme erledigt – dort gibt
      // es keinen späteren Abschluss, auf den man warten müsste.
      const settled =
        task.result_summary &&
        (task.result_summary.includes('abgeschlossen laut') ||
          task.result_summary.includes('Neustart angewiesen') ||
          task.result_summary.includes('Wartungsmodus'))

      if (settled) {
        if (inlineAction.value) {
          inlineAction.value = {
            ...inlineAction.value,
            message: task.result_summary,
            status: task.status === 'failed' ? 'failed' : 'succeeded',
          }
        }
        await load()
        return
      }
      if (task.status === 'failed') return
    } catch {
      return
    }
  }
}

function pollInlineAction(jobId: string, taskId: string) {
  if (inlinePoller) window.clearInterval(inlinePoller)

  inlinePoller = window.setInterval(async () => {
    try {
      const job = await api.job(jobId)
      if (!inlineAction.value) return

      inlineAction.value = {
        ...inlineAction.value,
        status: job.status,
        progress: job.progress,
        message: job.message ?? inlineAction.value.message,
      }

      if (job.status === 'succeeded' || job.status === 'failed') {
        if (inlinePoller) window.clearInterval(inlinePoller)
        inlinePoller = undefined
        quickTestBusy.value = null
        await load()
        followUpProof(taskId)
      }
    } catch {
      // Nächster Versuch beim nächsten Intervall.
    }
  }, 900)
}

/// Ordnet den Kunden einem Abrechnungskonto zu (oder löst die Zuordnung).
/// Danach neu laden, damit Beträge und Kontoanzeige sofort stimmen.
async function changeAccount(value: string) {
  const accountId = value || null
  accountBusy.value = true
  accountError.value = ''
  try {
    await api.setCustomerAccount(customerId.value, accountId)
    await load()
  } catch (caught) {
    accountError.value = errorMessage(caught)
  } finally {
    accountBusy.value = false
  }
}

function focusTimer() {
  timerSection.value?.scrollIntoView({ behavior: 'smooth', block: 'center' })
  timerSection.value?.querySelector('input')?.focus()
}

watch(customerId, load)
onMounted(() => {
  load()
  clock = window.setInterval(() => {
    now.value = Date.now()
  }, 1000)
})

onUnmounted(() => {
  if (clock) window.clearInterval(clock)
  if (inlinePoller) window.clearInterval(inlinePoller)
})
</script>

<template>
  <div class="page-stack">
    <div v-if="loading" class="skeleton-stack">
      <div class="skeleton skeleton--hero" />
      <div class="skeleton skeleton--panel" />
    </div>

    <div v-else-if="error" class="card empty-state">
      <ShieldAlert :size="26" />
      <p>{{ error }}</p>
      <button class="btn btn-primary" @click="load">Erneut versuchen</button>
    </div>

    <template v-else-if="detail">
      <div class="page-toolbar">
        <div class="breadcrumb">
          <RouterLink to="/kunden">Kunden</RouterLink>
          <ChevronRight :size="14" />
          <span>{{ detail.customer.name }}</span>
        </div>
        <RouterLink
          :to="`/kunden/${detail.customer.id}/abrechnung`"
          class="btn btn-ghost btn--sm"
          title="Noch nicht abgerechnete Zeiten als Aufstellung drucken"
        >
          <Printer :size="14" />
          Offene Zeiten drucken
        </RouterLink>
      </div>

      <section class="card customer-header">
        <div class="customer-identity">
          <div class="avatar avatar--large">{{ initials(detail.customer.name) }}</div>
          <div class="customer-copy">
            <div class="title-row">
              <h2>{{ detail.customer.name }}</h2>
              <span class="code-badge">{{ detail.customer.short_code }}</span>
            </div>
            <p class="customer-notes">{{ detail.customer.notes }}</p>
            <div class="contact-row">
              <span><Mail :size="14" /> {{ detail.customer.contact_email ?? '–' }}</span>
              <span><Phone :size="14" /> {{ detail.customer.phone ?? '–' }}</span>
              <span><Server :size="14" /> NinjaOne #{{ detail.customer.ninja_org_id ?? '–' }}</span>
            </div>
            <label class="account-picker">
              <Building2 :size="14" />
              <span>Abrechnungskonto</span>
              <select
                :value="detail.customer.account_id ?? ''"
                :disabled="accountBusy"
                @change="changeAccount(($event.target as HTMLSelectElement).value)"
              >
                <option value="">– keins (eigene Rechnung) –</option>
                <option v-for="account in accounts" :key="account.id" :value="account.id">
                  {{ account.code }} · {{ account.name }}
                </option>
              </select>
              <span v-if="detail.customer.account_code" class="account-tag">
                Rechnung über {{ detail.customer.account_code }}
              </span>
            </label>
            <p v-if="accountError" class="settings-message settings-message--error">
              {{ accountError }}
            </p>
          </div>
        </div>
        <div class="customer-badges">
          <span class="chip"><Building2 :size="14" /> {{ detail.contracts.length }} Vertrag</span>
          <span class="chip"><Globe2 :size="14" /> {{ detail.sites.length }} Site{{ detail.sites.length === 1 ? '' : 's' }}</span>
          <span class="chip"><HardDrive :size="14" /> {{ detail.devices.length }} Geräte</span>
        </div>
      </section>

      <section v-if="contract" class="metric-strip">
        <article class="metric-card">
          <p>Wartungspaket</p>
          <strong>{{ contract.name }}</strong>
          <small>monatlich · Abrechnung zum {{ contract.billing_day }}.</small>
        </article>
        <article class="metric-card">
          <p>Inklusivstunden</p>
          <strong>{{ formatHours(contract.included_hours) }}</strong>
          <small>{{ billingRow ? formatHours(billingRow.billable_hours) : '0 h' }} verbraucht</small>
          <ProgressBar :value="includedProgress" :tone="includedProgress > 90 ? 'amber' : 'teal'" />
        </article>
        <article class="metric-card">
          <p>Stundensatz</p>
          <strong>{{ currency(contract.hourly_rate) }}</strong>
          <small>für Stunden über dem Inklusivvolumen</small>
        </article>
        <article class="metric-card">
          <p>Pauschale</p>
          <strong>{{ currency(contract.flat_fee) }}</strong>
          <small>monatlich, unabhängig vom Aufwand</small>
        </article>
        <article class="metric-card metric-card--accent">
          <p>Abrechnung {{ monthLabel(month) }}</p>
          <strong>{{ billingRow ? currency(billingRow.total_amount) : '–' }}</strong>
          <small>
            {{ billingRow ? formatHours(billingRow.billable_hours) : '0 h' }} Leistung ·
            {{ billingRow?.overage_hours ? formatHours(billingRow.overage_hours) + ' Überhang' : 'im Inklusivvolumen' }}
          </small>
        </article>
      </section>

      <div class="detail-grid detail-grid--single">
        <section class="card">
          <header class="card-head">
            <div>
              <h2>Geräte</h2>
              <p>Aus NinjaOne synchronisiert</p>
            </div>
            <div class="head-actions">
              <span
                v-if="issueDevices.length"
                class="status-pill status-pill--warning"
                :title="`${issueDevices.length} von ${detail.devices.length} Geräten brauchen Aufmerksamkeit`"
              >
                <span class="badge-dot" />
                {{ issueDevices.length }} mit Handlungsbedarf
              </span>
              <span class="count-pill">{{ detail.devices.length }}</span>
            </div>
          </header>

          <section v-if="issueDevices.length" class="device-issues">
            <header class="device-issues-head">
              <ShieldAlert :size="15" />
              <strong>Handlungsbedarf</strong>
              <span>Klicken öffnet das Gerät</span>
            </header>

            <ul class="device-issues-list">
              <li
                v-for="device in issueDevices"
                :key="`issue-${device.id}`"
                class="device-issue"
                tabindex="0"
                @click="showDevice(device.id)"
                @keydown.enter="showDevice(device.id)"
              >
                <div class="device-icon" :class="{ 'device-icon--server': device.device_type === 'server' }">
                  <Server v-if="device.device_type === 'server'" :size="15" />
                  <HardDrive v-else :size="15" />
                </div>
                <div class="device-issue-copy">
                  <strong>{{ device.hostname }}</strong>
                  <small>{{ device.os ?? 'unbekanntes System' }}</small>
                </div>
                <span class="device-issue-reason">{{ issueReason(device) }}</span>
                <ChevronRight :size="15" class="device-issue-chevron" />
              </li>
            </ul>
          </section>

          <div v-if="detail.devices.length" class="device-picker">
            <label class="field">
              <span>Gerät</span>
              <select v-model="selectedDeviceId" aria-label="Gerät wählen">
                <option v-for="device in detail.devices" :key="device.id" :value="device.id">
                  {{ device.hostname }} · {{ device.os ?? 'unbekannt' }}{{ device.online ? '' : ' (offline)' }}
                </option>
              </select>
            </label>
            <button
              class="btn btn-primary"
              :disabled="!selectedDeviceId"
              @click="showDevice()"
            >
              <Eye :size="15" />
              Anzeigen
            </button>
          </div>

          <article v-if="shownDevice" class="device-panel">
            <header class="device-panel-head">
              <div class="device-panel-title">
                <div class="device-icon" :class="{ 'device-icon--server': shownDevice.device_type === 'server' }">
                  <Server v-if="shownDevice.device_type === 'server'" :size="17" />
                  <HardDrive v-else :size="17" />
                </div>
                <div>
                  <strong>{{ shownDevice.hostname }}</strong>
                  <small>{{ shownDevice.os ?? 'unbekanntes System' }}</small>
                </div>
              </div>
              <button class="icon-btn" title="Schließen" @click="closeDevice">
                <X :size="16" />
              </button>
            </header>

            <div class="device-panel-facts">
              <div class="fact">
                <span class="fact-label">Patchstand</span>
                <span
                  class="status-pill"
                  :class="{
                    'status-pill--warning': shownDevice.patch_status === 'pending',
                    'status-pill--ok': shownDevice.patch_status === 'ok',
                    'status-pill--muted': shownDevice.patch_status !== 'pending' && shownDevice.patch_status !== 'ok',
                  }"
                >
                  <span class="badge-dot" />
                  {{
                    shownDevice.patch_status === 'pending'
                      ? 'Patches offen'
                      : shownDevice.patch_status === 'ok'
                        ? 'aktuell'
                        : 'unbekannt'
                  }}
                </span>
                <small class="fact-hint">{{ patchSummary(shownDevice) }}</small>
              </div>
              <div class="fact">
                <span class="fact-label">Antivirus</span>
                <span
                  class="status-pill"
                  :class="{
                    'status-pill--ok': shownDevice.antivirus_status === 'ok',
                    'status-pill--warning': shownDevice.antivirus_status === 'warning',
                    'status-pill--muted': shownDevice.antivirus_status !== 'ok' && shownDevice.antivirus_status !== 'warning',
                  }"
                >
                  <span class="badge-dot" />
                  {{
                    shownDevice.antivirus_status === 'ok'
                      ? 'ok'
                      : shownDevice.antivirus_status === 'warning'
                        ? 'prüfen'
                        : 'unbekannt'
                  }}
                </span>
                <small class="fact-hint">{{ antivirusDetail(shownDevice) }}</small>
              </div>
              <div class="fact">
                <span class="fact-label">Status</span>
                <span class="online-state" :class="{ 'is-offline': !shownDevice.online }">
                  <Wifi v-if="shownDevice.online" :size="13" />
                  <WifiOff v-else :size="13" />
                  {{ shownDevice.online ? 'online' : 'offline' }}
                </span>
              </div>
              <div class="fact">
                <span class="fact-label">Letzter Kontakt</span>
                <strong>{{ shownDevice.last_seen ? formatDateTime(shownDevice.last_seen) : '–' }}</strong>
              </div>
              <div class="fact fact--teamviewer">
                <span class="fact-label">TeamViewer</span>
                <div v-if="teamviewerEditing" class="tv-edit">
                  <input
                    v-model="teamviewerDraft"
                    inputmode="numeric"
                    placeholder="123 456 789"
                    @keyup.enter="saveTeamviewer"
                    @keyup.esc="teamviewerEditing = false"
                  />
                  <button class="btn btn-primary btn-sm" :disabled="teamviewerBusy" @click="saveTeamviewer">
                    {{ teamviewerBusy ? '…' : 'Speichern' }}
                  </button>
                  <button class="icon-btn" title="Abbrechen" @click="teamviewerEditing = false">
                    <X :size="13" />
                  </button>
                </div>
                <div v-else class="tv-read">
                  <strong v-if="shownDevice.teamviewer_id" class="tv-id">
                    {{ shownDevice.teamviewer_id }}
                  </strong>
                  <span v-else class="tv-empty">nicht hinterlegt</span>
                  <button class="tv-edit-btn" @click="editTeamviewer">
                    {{ shownDevice.teamviewer_id ? 'ändern' : 'hinterlegen' }}
                  </button>
                </div>
              </div>
            </div>

            <section v-if="patches.length" class="patch-box" :class="{ 'is-open': patchesOpen }">
              <button class="patch-box-head" @click="patchesOpen = !patchesOpen">
                <ShieldCheck :size="14" />
                <strong>{{ patches.length }} Patches gemeldet</strong>
                <span class="patch-box-summary">{{ patchSummary(shownDevice) }}</span>
                <ChevronRight :size="15" class="patch-box-chevron" />
              </button>

              <div v-if="patchesOpen" class="patch-list">
                <div v-if="softwarePatchCount" class="patch-tabs">
                  <button
                    class="patch-tab"
                    :class="{ 'is-active': patchFilter === 'os' }"
                    @click="patchFilter = 'os'"
                  >
                    Betriebssystem
                    <span>{{ osPatchCount }}</span>
                  </button>
                  <button
                    class="patch-tab"
                    :class="{ 'is-active': patchFilter === 'software' }"
                    @click="patchFilter = 'software'"
                  >
                    Software
                    <span>{{ softwarePatchCount }}</span>
                  </button>
                </div>

                <div
                  v-for="patch in visiblePatches"
                  :key="patch.patch_id"
                  class="patch-row"
                  :class="`patch-row--${patch.status.toLowerCase()}`"
                >
                  <span class="patch-state">{{ patchStatusLabel(patch.status) }}</span>
                  <div class="patch-copy">
                    <span>{{ patch.name }}</span>
                    <small>{{ patchMeta(patch) }}</small>
                  </div>
                </div>

                <p v-if="!visiblePatches.length" class="patch-empty">
                  Keine {{ patchFilter === 'os' ? 'Betriebssystem' : 'Software-' }}Patches gemeldet.
                </p>
              </div>
            </section>

            <p class="panel-hint">
              Wenn du vorhast zu arbeiten: <strong>starte zuerst die Zeit</strong>, wähle die
              Aufgabe, führe sie durch und schreibe abschließend deinen Kommentar.
            </p>

            <div class="device-panel-actions">
              <button
                v-if="!deviceTimerRunning"
                class="btn btn-primary btn-sm"
                :disabled="quickTestBusy !== null"
                @click="startDeviceTimer"
              >
                <Play :size="14" />
                Timer starten
              </button>
              <template v-else>
                <span class="timer-inline">
                  <span class="timer-dot" />
                  <strong>{{ formatTimer(deviceElapsed) }}</strong>
                </span>
                <button class="btn btn-ghost btn-sm" @click="stopTimer">
                  <Square :size="13" />
                  Stop
                </button>
              </template>
              <a
                v-if="remoteUrl(shownDevice.ninja_device_id)"
                class="btn btn-ghost btn-sm"
                :href="remoteUrl(shownDevice.ninja_device_id)!"
                target="_blank"
                rel="noreferrer"
                title="Gerät in NinjaOne öffnen – dort Remote-Session starten"
              >
                <MonitorSmartphone :size="14" />
                Remote
              </a>
              <a
                v-if="teamviewerLink(shownDevice.teamviewer_id)"
                class="btn btn-ghost btn-sm"
                :href="teamviewerLink(shownDevice.teamviewer_id)!"
                :title="`TeamViewer mit ${shownDevice.hostname} verbinden (ID ${shownDevice.teamviewer_id})`"
              >
                <MonitorPlay :size="14" />
                TeamViewer
              </a>
            </div>

            <section class="device-actions">
              <header class="device-actions-head">
                <strong>Am Gerät ausführen</strong>
                <small v-if="!shownDevice.online">Gerät ist offline – Aufträge warten auf den nächsten Kontakt.</small>
              </header>

              <div class="action-group">
                <span class="action-group-label">Prüfen</span>
                <div class="action-row">
                  <button
                    class="btn btn-ghost btn-sm"
                    :disabled="quickTestBusy !== null"
                    @click="askAction('patch-scan')"
                  >
                    <Wrench :size="14" />
                    {{ quickTestBusy === 'patch-scan' ? 'läuft …' : 'OS-Scan' }}
                  </button>
                  <button
                    class="btn btn-ghost btn-sm"
                    :disabled="quickTestBusy !== null"
                    @click="askAction('software-scan')"
                  >
                    <PackageSearch :size="14" />
                    {{ quickTestBusy === 'software-scan' ? 'läuft …' : 'Software-Scan' }}
                  </button>
                </div>
              </div>

              <div class="action-group">
                <span class="action-group-label">Installieren</span>
                <div class="action-row">
                  <button
                    class="btn btn-ghost btn-sm"
                    :disabled="quickTestBusy !== null"
                    @click="askAction('patch-apply')"
                  >
                    <ArrowDownToLine :size="14" />
                    {{ quickTestBusy === 'patch-apply' ? 'läuft …' : 'OS-Updates' }}
                  </button>
                  <button
                    class="btn btn-ghost btn-sm"
                    :disabled="quickTestBusy !== null"
                    @click="askAction('software-apply')"
                  >
                    <ArrowDownToLine :size="14" />
                    {{ quickTestBusy === 'software-apply' ? 'läuft …' : 'Software-Updates' }}
                  </button>
                </div>
              </div>

              <div class="action-group">
                <span class="action-group-label">Gerät</span>
                <div class="action-row">
                  <button
                    class="btn btn-ghost btn-sm"
                    :disabled="quickTestBusy !== null"
                    @click="askAction('reboot')"
                  >
                    <Power :size="14" />
                    {{ quickTestBusy === 'reboot' ? 'läuft …' : 'Neustart' }}
                  </button>
                  <button
                    v-if="!maintenanceActive"
                    class="btn btn-ghost btn-sm"
                    :disabled="quickTestBusy !== null"
                    @click="askAction('maintenance-start')"
                  >
                    <BellOff :size="14" />
                    Wartungsmodus
                  </button>
                  <button
                    v-else
                    class="btn btn-ghost btn-sm"
                    :disabled="quickTestBusy !== null"
                    @click="askAction('maintenance-stop')"
                  >
                    <BellRing :size="14" />
                    Modus beenden
                  </button>
                </div>
                <small v-if="maintenanceActive" class="action-note">
                  Wartungsmodus bis {{ maintenanceUntilLabel }} – Alarme sind unterdrückt.
                </small>
              </div>

              <div class="action-group">
                <span class="action-group-label">Ohne Rückfrage</span>
                <div class="action-row">
                  <button
                    class="btn btn-ghost btn-sm"
                    :disabled="quickTestBusy !== null"
                    @click="askAction('script')"
                  >
                    <Terminal :size="14" />
                    {{ quickTestBusy === 'script' ? 'läuft …' : 'Script-Test' }}
                  </button>
                </div>
              </div>
            </section>

            <div v-if="inlineAction" class="inline-action" :class="`inline-action--${inlineAction.status}`">
              <div class="inline-action-head">
                <strong>{{ inlineAction.title }}</strong>
                <span
                  class="status-pill"
                  :class="inlineAction.status === 'succeeded' ? 'status-pill--ok' : inlineAction.status === 'failed' ? 'status-pill--danger' : 'status-pill--warning'"
                >
                  {{ inlineAction.status === 'succeeded' ? 'OK' : inlineAction.status === 'failed' ? 'Fehler' : `${inlineAction.progress} %` }}
                </span>
              </div>
              <div v-if="inlineAction.status === 'queued' || inlineAction.status === 'running'" class="progress">
                <div class="progress-fill tone-teal" :style="{ width: `${Math.max(4, inlineAction.progress)}%` }" />
              </div>
              <p class="inline-action-msg">{{ inlineAction.message ?? 'Wird vorbereitet …' }}</p>
              <button class="inline-action-close" title="Ausblenden" @click="inlineAction = null">
                <X :size="13" />
              </button>
            </div>

            <div class="device-timer" ref="timerSection">
              <div class="device-timer-head">
                <div>
                  <strong>Zeit auf diesem Gerät</strong>
                  <p>
                    Tätigkeit und Kommentar für den Monatsbericht. Gestartet wird oben über
                    „Timer starten".
                  </p>
                </div>
              </div>

              <div class="device-timer-form">
                <label class="field">
                  <span>Tätigkeit</span>
                  <input
                    v-model="deviceActivity"
                    type="text"
                    placeholder="z. B. Gerätewartung"
                    :disabled="deviceTimerRunning"
                  />
                </label>
                <label class="field">
                  <span>Kommentar</span>
                  <input
                    v-model="deviceComment"
                    type="text"
                    placeholder="Was wurde gemacht?"
                    :disabled="deviceTimerRunning"
                  />
                </label>
              </div>
            </div>
          </article>
        </section>
      </div>

      <section class="card">
        <header class="card-head">
          <div>
            <h2>Wartungsläufe</h2>
            <p>Letzte und kommende Läufe für diesen Kunden</p>
          </div>
          <RouterLink to="/wartungen" class="link">Alle Wartungen</RouterLink>
        </header>
        <div v-if="detail.runs.length" class="run-grid">
          <RunCard v-for="run in detail.runs" :key="run.id" :run="run" />
        </div>
        <div v-else class="empty-state empty-state--compact"><p>Noch keine Wartungsläufe.</p></div>
      </section>

      <section class="card">
        <header class="card-head">
          <div>
            <h2>Letzte Zeiten</h2>
            <p>{{ formatDuration(totalRecentTime) }} in den letzten Einträgen</p>
          </div>
          <div class="head-actions">
            <RouterLink
              :to="`/kunden/${detail.customer.id}/abrechnung`"
              class="btn btn-ghost btn--sm"
            >
              <Printer :size="14" />
              Abrechnung
            </RouterLink>
            <RouterLink to="/zeiten" class="link">Alle Zeiten</RouterLink>
          </div>
        </header>

        <div v-if="detail.recent_time.length" class="table-wrap">
          <table class="data-table data-table--compact">
            <thead>
              <tr>
                <th>Datum</th>
                <th>Leistung</th>
                <th>Quelle</th>
                <th class="num">Dauer</th>
                <th>Abrechenbar</th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="entry in detail.recent_time" :key="entry.id">
                <td>{{ formatDate(entry.started_at) }}</td>
                <td>
                  <div class="stacked-cell">
                    <span>{{ entry.activity }}</span>
                    <small>{{ entry.device_label ?? entry.task_title ?? entry.note }}</small>
                  </div>
                </td>
                <td><StatusBadge :value="entry.source" kind="source" /></td>
                <td class="num">{{ formatDuration(entry.duration_seconds) }}</td>
                <td>
                  <span class="status-pill" :class="entry.billable ? 'status-pill--ok' : 'status-pill--muted'">
                    {{ entry.billable ? 'ja' : 'nein' }}
                  </span>
                </td>
              </tr>
            </tbody>
          </table>
        </div>
        <div v-else class="empty-state empty-state--compact"><p>Noch keine Zeiten erfasst.</p></div>
      </section>
    </template>

    <Modal
      :open="pendingAction !== null"
      :title="pendingAction ? ACTION_TITLES[pendingAction] : ''"
      :subtitle="shownDevice?.hostname"
      @close="pendingAction = null"
    >
      <div v-if="pendingAction === 'patch-apply'" class="confirm-body">
        <p>
          NinjaOne installiert jetzt alle <strong>genehmigten Betriebssystem-Updates</strong> auf
          {{ shownDevice?.hostname }}. Je nach Update ist danach ein Neustart nötig – bei
          Servern also lieber im Wartungsfenster.
        </p>
        <p class="confirm-hint">
          Abgelehnte Updates bleiben unberührt. Die Installation läuft im Hintergrund weiter,
          auch wenn du dieses Fenster schließt.
        </p>
      </div>

      <div v-else-if="pendingAction === 'software-apply'" class="confirm-body">
        <p>
          NinjaOne installiert jetzt alle <strong>genehmigten Software-Updates</strong> auf
          {{ shownDevice?.hostname }}. Programme können sich dabei schließen; offene Dokumente
          sollten gespeichert sein.
        </p>
        <p class="confirm-hint">
          Der Anwender bekommt keine Vorwarnung – bei laufender Arbeit lieber vorher anrufen.
        </p>
      </div>

      <div v-else-if="pendingAction === 'reboot'" class="confirm-body">
        <p>
          {{ shownDevice?.hostname }} wird neu gestartet. Wer gerade am Gerät arbeitet, verliert
          nicht gespeicherte Arbeit.
        </p>
        <label class="field">
          <span>Begründung (erscheint im NinjaOne-Protokoll)</span>
          <input v-model="rebootReason" type="text" maxlength="120" />
        </label>
        <label class="check-row">
          <input v-model="rebootForced" type="checkbox" />
          <span>Erzwingen – Anwendungen werden ohne Nachfrage geschlossen</span>
        </label>
      </div>

      <div v-else-if="pendingAction === 'maintenance-start'" class="confirm-body">
        <p>
          Alarme für {{ shownDevice?.hostname }} werden unterdrückt, solange das Wartungsfenster
          läuft. Patch-Läufe und AV-Scans in NinjaOne laufen normal weiter.
        </p>
        <label class="field">
          <span>Dauer</span>
          <select v-model.number="maintenanceMinutes">
            <option :value="15">15 Minuten</option>
            <option :value="30">30 Minuten</option>
            <option :value="60">1 Stunde</option>
            <option :value="120">2 Stunden</option>
            <option :value="240">4 Stunden</option>
          </select>
        </label>
      </div>

      <div v-else-if="pendingAction === 'maintenance-stop'" class="confirm-body">
        <p>Das Wartungsfenster wird beendet. Alarme greifen ab sofort wieder.</p>
      </div>

      <template #footer>
        <button class="btn btn-ghost" @click="pendingAction = null">Abbrechen</button>
        <button
          class="btn"
          :class="pendingAction === 'patch-apply' || pendingAction === 'software-apply' || pendingAction === 'reboot' ? 'btn-danger' : 'btn-primary'"
          @click="confirmAction"
        >
          {{ ACTION_TITLES[pendingAction ?? 'patch-scan'] }} ausführen
        </button>
      </template>
    </Modal>
  </div>
</template>
