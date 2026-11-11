<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import {
  ArrowLeft,
  ChevronRight,
  Clock3,
  Copy,
  ExternalLink,
  Globe,
  Eye,
  EyeOff,
  Globe2,
  HeartPulse,
  KeyRound,
  MonitorSmartphone,
  Package,
  Pencil,
  Play,
  Plus,
  Printer,
  Save,
  ShieldAlert,
  MessageSquarePlus,
  RefreshCw,
  Square,
  Timer,
  Trash2,
  TriangleAlert,
  Upload,
} from 'lucide-vue-next'
import { api, errorMessage } from '../api'
import type {
  Account,
  CreateWebSitePayload,
  Site,
  TimeEntry,
  UpdateWebSitePayload,
  WebAction,
  WebClientDetail,
} from '../types'
import {
  currency,
  formatDate,
  formatDateTime,
  formatDuration,
  formatHours,
  formatTimer,
  initials,
  relativeTime,
  sourceLabel,
  taskStatusLabel,
} from '../format'
import Modal from '../components/Modal.vue'
import StatusBadge from '../components/StatusBadge.vue'
import { useAppStore } from '../stores/app'

const route = useRoute()
const router = useRouter()
const store = useAppStore()

const detail = ref<WebClientDetail | null>(null)
const accounts = ref<Account[]>([])
const accountBusy = ref(false)
const accountError = ref('')
const loading = ref(true)
const error = ref('')
const siteModalOpen = ref(false)
const editingSiteId = ref<string | null>(null)
const deletingSiteId = ref<string | null>(null)
const clientBusy = ref(false)
const deleteClientOpen = ref(false)
const deleteClientError = ref('')

// Vorschau: welche Website wird gezeigt, welche nicht (vom Hoster blockiert)
const hiddenPreviews = ref<Record<string, boolean>>({})

// Kommentar nach dem Bearbeiten einer Website
const commentModalOpen = ref(false)
const modalComment = ref('')
const commentMinutes = ref(15)
const commentSaving = ref(false)
const commentContext = ref('')
const siteSaving = ref(false)
const siteError = ref('')
const actionBusy = ref<string | null>(null)
const revealPassword = ref<Record<string, boolean>>({})
const commentDraft = ref<Record<string, string>>({})
const savingComment = ref<string | null>(null)
const timerActivity = ref('Website-Betreuung')
const timerComment = ref('')
const timerSiteId = ref('')
const manualMinutes = ref(15)
const savingManual = ref(false)

/** Vorschläge für die Tätigkeit – das Feld bleibt frei beschreibbar. */
const webActivities = [
  'Website-Betreuung',
  'Plugin-Pflege',
  'Core-Update',
  'Health-Check',
  'Backup',
  'Fehlersuche',
  'Kundenwunsch',
]
const now = ref(Date.now())
const runningAction = ref<WebAction | null>(null)
let clock: number | undefined
let actionPoller: number | undefined

const clientId = computed(() => route.params.id as string)
const elapsed = computed(() => {
  const timer = store.activeTimer
  if (!timer || timer.web_client_id !== clientId.value) return 0
  const started = new Date(timer.started_at).getTime()
  if (Number.isNaN(started)) return 0
  return Math.max(0, Math.floor((now.value - started) / 1000))
})
const isOurTimer = computed(
  () => !!store.activeTimer && store.activeTimer.web_client_id === clientId.value,
)

/** Der globale Stopp-Dialog übernimmt, wenn ein Timer für diesen Kunden läuft. */
const timerForThisClient = computed(() => (isOurTimer.value ? store.activeTimer : null))

const siteForm = ref<CreateWebSitePayload>({
  url: '',
  label: '',
  environment: 'prod',
  agent_user: '',
  agent_password: '',
  agent_secret: '',
  login_url: '',
  login_user: '',
  login_password: '',
  login_note: '',
  notes: '',
})

function kindLabel(kind: string): string {
  return { health: 'Health-Check', plugins: 'Plugin-Update', core: 'Core-Update' }[kind] ?? kind
}

function kindIcon(kind: string) {
  if (kind === 'health') return HeartPulse
  if (kind === 'plugins') return Package
  return Upload
}

function actionTone(status: string): 'teal' | 'amber' | 'red' {
  if (status === 'succeeded') return 'teal'
  if (status === 'failed') return 'red'
  return 'amber'
}

async function load(initial = false) {
  if (initial) loading.value = true
  try {
    const [client, accountList] = await Promise.all([
      api.webClient(clientId.value),
      api.accounts().catch(() => [] as Account[]),
    ])
    accounts.value = accountList
    detail.value = client
    error.value = ''
    for (const entry of detail.value.time_entries) {
      commentDraft.value[entry.id] = entry.note ?? ''
    }
  } catch (caught) {
    if (!error.value) error.value = errorMessage(caught)
  } finally {
    loading.value = false
  }
}

/// Ordnet den Web-Kunden einem Abrechnungskonto zu (oder löst die Zuordnung).
async function changeAccount(value: string) {
  accountBusy.value = true
  accountError.value = ''
  try {
    await api.setWebClientAccount(clientId.value, value || null)
    await load()
  } catch (caught) {
    accountError.value = errorMessage(caught)
  } finally {
    accountBusy.value = false
  }
}

function emptySiteForm(): CreateWebSitePayload {
  return {
    url: '',
    label: '',
    environment: 'prod',
    agent_user: '',
    agent_password: '',
    agent_secret: '',
    login_url: '',
    login_user: '',
    login_password: '',
    login_note: '',
    notes: '',
  }
}

function openSiteModal() {
  editingSiteId.value = null
  siteForm.value = emptySiteForm()
  siteError.value = ''
  siteModalOpen.value = true
}

/** Öffnet dasselbe Formular, aber gefüllt – Passwörter bleiben absichtlich leer. */
function editSite(site: Site) {
  editingSiteId.value = site.id
  siteForm.value = {
    url: site.url,
    label: site.label ?? '',
    environment: (site.environment as 'prod' | 'staging' | 'local') ?? 'prod',
    agent_user: site.agent_user ?? '',
    agent_password: '',
    agent_secret: '',
    login_url: site.login_url ?? '',
    login_user: site.login_user ?? '',
    login_password: '',
    login_note: site.login_note ?? '',
    notes: site.notes ?? '',
  }
  siteError.value = ''
  siteModalOpen.value = true
}

async function removeSite(site: Site) {
  const hint =
    'Die Website wird entfernt, ihre Zugangsdaten und der Agent-Zugang ebenfalls. ' +
    'Bereits erfasste Zeiten bleiben erhalten.'
  if (!window.confirm(`Website „${site.label ?? site.url}" wirklich löschen?

${hint}`)) return

  deletingSiteId.value = site.id
  try {
    await api.deleteWebSite(site.id)
    store.toast('Website gelöscht.', 'success')
    await load()
  } catch (caught) {
    store.toast(errorMessage(caught), 'error')
  } finally {
    deletingSiteId.value = null
  }
}

async function saveSite() {
  if (!siteForm.value.url.trim()) {
    siteError.value = 'Bitte die Internetadresse angeben.'
    return
  }

  siteSaving.value = true
  siteError.value = ''
  const wasEditing = !!editingSiteId.value
  const editedLabel = siteForm.value.label || siteForm.value.url

  try {
    if (editingSiteId.value) {
      await api.updateWebSite(editingSiteId.value, buildUpdatePayload())
      store.toast('Website geändert.', 'success')
    } else {
      await api.createWebSite(clientId.value, siteForm.value)
      store.toast('Website angelegt.', 'success')
    }
    siteModalOpen.value = false
    await load()

    // Nach dem Bearbeiten gehört eine Begründung in die Abrechnung. Läuft schon
    // ein Timer für diesen Kunden, fragt der Stopp-Dialog später danach – zwei
    // Fenster übereinander wären nur verwirrend.
    if (wasEditing) {
      if (timerForThisClient.value) {
        store.toast('Kommentar wird beim Stoppen des Timers abgefragt.', 'info')
      } else {
        askForComment(editedLabel)
      }
    }
  } catch (caught) {
    siteError.value = errorMessage(caught)
  } finally {
    siteSaving.value = false
  }
}

/**
 * Schickt nur, was sich geändert hat. Leere Passwortfelder bleiben draußen –
 * sonst würde ein geleertes Feld das gespeicherte Geheimnis löschen.
 */
function buildUpdatePayload(): UpdateWebSitePayload {
  const form = siteForm.value
  const payload: UpdateWebSitePayload = {
    url: form.url,
    environment: form.environment,
    label: form.label ?? '',
    agent_user: form.agent_user ?? '',
    login_url: form.login_url ?? '',
    login_user: form.login_user ?? '',
    login_note: form.login_note ?? '',
    notes: form.notes ?? '',
  }

  if (form.agent_password) payload.agent_password = form.agent_password
  if (form.agent_secret) payload.agent_secret = form.agent_secret
  if (form.login_password) payload.login_password = form.login_password

  return payload
}

/** Deaktivieren blendet den Kunden aus, ohne Zeiten oder Stammdaten anzufassen. */
async function setClientActive(active: boolean) {
  clientBusy.value = true
  try {
    await api.updateWebClient(clientId.value, { active })
    store.toast(active ? 'Web-Kunde wieder aktiv.' : 'Web-Kunde deaktiviert.', 'success')
    if (!active) {
      router.push('/websites')
      return
    }
    await load()
  } catch (caught) {
    store.toast(errorMessage(caught), 'error')
  } finally {
    clientBusy.value = false
  }
}

async function deleteClient() {
  deleteClientError.value = ''
  clientBusy.value = true
  try {
    await api.deleteWebClient(clientId.value)
    store.toast('Web-Kunde gelöscht.', 'success')
    deleteClientOpen.value = false
    router.push('/websites')
  } catch (caught) {
    // Meist „hat Zeiteinträge" – die Meldung ist für den Nutzer gedacht.
    deleteClientError.value = errorMessage(caught)
  } finally {
    clientBusy.value = false
  }
}

/** Öffnet die Website in einem neuen Tab und startet dabei die Zeit. */
async function openAndTrack(site: Site) {
  window.open(site.url, '_blank', 'noopener')

  const running = store.activeTimer
  if (running) {
    // Nicht stillschweigend umbuchen: laufende Zeit bleibt, wo sie ist.
    if (running.web_client_id !== clientId.value) {
      store.toast(
        `Timer läuft noch für ${running.customer_short_code ?? 'einen anderen Kunden'} – bitte zuerst stoppen.`,
        'info',
      )
      return
    }
    if (running.web_client_id === clientId.value) {
      store.toast('Timer läuft bereits für diesen Kunden.', 'info')
    }
    return
  }

  timerSiteId.value = site.id
  await startTimer()
}

function togglePreview(site: Site) {
  hiddenPreviews.value = {
    ...hiddenPreviews.value,
    [site.id]: !hiddenPreviews.value[site.id],
  }
}

/** Kurzer Hostname für die Vorschauleiste. */
function siteHost(url: string): string {
  try {
    return new URL(url).host
  } catch {
    return url
  }
}

/** Öffnet den Kommentar-Dialog; die Dauer gilt nur ohne laufenden Timer. */
function askForComment(siteLabel: string) {
  commentContext.value = siteLabel
  modalComment.value = ''
  commentMinutes.value = 15
  commentModalOpen.value = true
}

async function saveCommentModal() {
  const text = modalComment.value.trim()
  const running = timerForThisClient.value

  if (!text) {
    commentModalOpen.value = false
    return
  }

  commentSaving.value = true
  try {
    if (running) {
      await api.setTimeComment(running.id, text)
      store.toast('Kommentar gespeichert.', 'success')
    } else {
      await api.createWebTimeEntry({
        web_client_id: clientId.value,
        site_id: timerSiteId.value || null,
        activity: timerActivity.value.trim() || 'Website-Betreuung',
        comment: text,
        minutes: commentMinutes.value,
        billable: true,
      })
      store.toast('Zeit mit Kommentar gespeichert.', 'success')
    }
    commentModalOpen.value = false
    await load()
  } catch (caught) {
    store.toast(errorMessage(caught), 'error')
  } finally {
    commentSaving.value = false
  }
}

async function runAction(siteId: string, kind: 'health' | 'plugins' | 'core') {
  actionBusy.value = `${siteId}:${kind}`
  try {
    const action = await api.runWebAction(siteId, kind, true)
    runningAction.value = action
    store.toast(`${kindLabel(kind)} gestartet.`, 'info')
    pollAction(action.id)
  } catch (caught) {
    actionBusy.value = null
    store.toast(errorMessage(caught), 'error')
  }
}

function pollAction(actionId: string) {
  if (actionPoller) window.clearInterval(actionPoller)
  actionPoller = window.setInterval(async () => {
    try {
      const action = await api.webAction(actionId)
      runningAction.value = action
      if (action.status === 'succeeded' || action.status === 'failed') {
        if (actionPoller) window.clearInterval(actionPoller)
        actionPoller = undefined
        actionBusy.value = null
        store.toast(
          action.status === 'succeeded'
            ? `${kindLabel(action.kind)} abgeschlossen.`
            : `${kindLabel(action.kind)} fehlgeschlagen.`,
          action.status === 'succeeded' ? 'success' : 'error',
        )
        await load()
      }
    } catch {
      // Nächster Versuch beim nächsten Intervall.
    }
  }, 900)
}

async function startTimer() {
  try {
    await api.startWebTimer({
      web_client_id: clientId.value,
      site_id: timerSiteId.value || null,
      activity: timerActivity.value,
      comment: timerComment.value,
    })
    await store.loadTimer()
    timerComment.value = ''
    store.toast('Timer läuft.', 'success')
  } catch (caught) {
    store.toast(errorMessage(caught), 'error')
  }
}

/** Trägt eine Zeit nachträglich mit fester Dauer ein – ohne laufenden Timer. */
async function saveManualEntry() {
  const activity = timerActivity.value.trim()
  if (!activity) {
    store.toast('Bitte eine Tätigkeit angeben.', 'error')
    return
  }
  if (!manualMinutes.value || manualMinutes.value < 1) {
    store.toast('Bitte eine Dauer in Minuten angeben.', 'error')
    return
  }

  savingManual.value = true
  try {
    await api.createWebTimeEntry({
      web_client_id: clientId.value,
      site_id: timerSiteId.value || null,
      activity,
      comment: timerComment.value.trim() || null,
      minutes: manualMinutes.value,
      billable: true,
    })
    store.toast('Zeit gespeichert.', 'success')
    timerComment.value = ''
    await load()
  } catch (caught) {
    store.toast(errorMessage(caught), 'error')
  } finally {
    savingManual.value = false
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

async function saveComment(entry: TimeEntry) {
  const draft = commentDraft.value[entry.id] ?? ''
  if (draft === (entry.note ?? '')) return
  savingComment.value = entry.id
  try {
    await api.setTimeComment(entry.id, draft)
    store.toast('Kommentar gespeichert.', 'success')
    await load()
  } catch (caught) {
    store.toast(errorMessage(caught), 'error')
  } finally {
    savingComment.value = null
  }
}

function togglePassword(siteId: string) {
  revealPassword.value = {
    ...revealPassword.value,
    [siteId]: !revealPassword.value[siteId],
  }
}

async function copy(value: string | null, label: string) {
  if (!value) return
  try {
    await navigator.clipboard.writeText(value)
    store.toast(`${label} kopiert.`, 'success')
  } catch {
    store.toast('Kopieren nicht möglich.', 'error')
  }
}

onMounted(async () => {
  await store.loadTimer()
  await load(true)
  clock = window.setInterval(() => {
    now.value = Date.now()
  }, 1000)
})

onUnmounted(() => {
  if (clock) window.clearInterval(clock)
  if (actionPoller) window.clearInterval(actionPoller)
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
      <button class="btn btn-primary" @click="load(true)">Erneut versuchen</button>
    </div>

    <template v-else-if="detail">
      <div class="page-toolbar">
        <div class="breadcrumb">
          <RouterLink to="/websites">Websites</RouterLink>
          <ChevronRight :size="14" />
          <span>{{ detail.client.name }}</span>
        </div>
        <RouterLink
          :to="`/websites/${detail.client.id}/abrechnung`"
          class="btn btn-ghost btn--sm"
          title="Noch nicht abgerechnete Zeiten als Aufstellung drucken"
        >
          <Printer :size="14" />
          Offene Zeiten drucken
        </RouterLink>
      </div>

      <section class="card customer-header">
        <div class="customer-identity">
          <div class="avatar avatar--large">{{ initials(detail.client.name) }}</div>
          <div class="customer-copy">
            <div class="title-row">
              <h2>{{ detail.client.name }}</h2>
              <span class="code-badge">{{ detail.client.short_code }}</span>
            </div>
            <p v-if="detail.client.notes" class="customer-notes">{{ detail.client.notes }}</p>
            <div class="contact-row">
              <span>{{ detail.client.contact_name ?? '–' }}</span>
              <span>{{ detail.client.contact_email ?? '–' }}</span>
              <span>{{ detail.client.phone ?? '–' }}</span>
              <span>{{ currency(detail.client.hourly_rate) }} / Stunde</span>
            </div>
            <label class="account-picker">
              <Globe2 :size="14" />
              <span>Abrechnungskonto</span>
              <select
                :value="detail.client.account_id ?? ''"
                :disabled="accountBusy"
                @change="changeAccount(($event.target as HTMLSelectElement).value)"
              >
                <option value="">– keins (eigene Rechnung) –</option>
                <option v-for="account in accounts" :key="account.id" :value="account.id">
                  {{ account.code }} · {{ account.name }}
                </option>
              </select>
              <span v-if="detail.client.account_code" class="account-tag">
                Rechnung über {{ detail.client.account_code }}
              </span>
            </label>
            <p v-if="accountError" class="settings-message settings-message--error">
              {{ accountError }}
            </p>
          </div>
        </div>
        <div class="customer-badges">
          <span v-if="!detail.client.active" class="status-pill status-pill--muted">
            <span class="badge-dot" />
            deaktiviert
          </span>
          <span class="chip"><Globe2 :size="14" /> {{ detail.sites.length }} Website{{ detail.sites.length === 1 ? '' : 's' }}</span>
          <span class="chip"><Clock3 :size="14" /> {{ formatHours(detail.month_hours) }} diesen Monat</span>
          <span class="chip"><Timer :size="14" /> {{ formatHours(detail.total_hours) }} gesamt</span>
          <button
            class="icon-btn"
            :title="detail.client.active ? 'Web-Kunde deaktivieren' : 'Web-Kunde wieder aktivieren'"
            :disabled="clientBusy"
            @click="setClientActive(!detail.client.active)"
          >
            <Eye v-if="detail.client.active" :size="16" />
            <EyeOff v-else :size="16" />
          </button>
          <button
            class="icon-btn icon-btn--danger"
            title="Web-Kunde löschen"
            @click="deleteClientOpen = true"
          >
            <Trash2 :size="16" />
          </button>
        </div>
      </section>

      <section class="card timer-bar">
        <div class="timer-bar-head">
          <div>
            <h2>Zeit erfassen</h2>
            <p>Live messen oder nachträglich erfassen – der Kommentar erscheint im Monatsbericht.</p>
          </div>
          <div v-if="isOurTimer" class="timer-live">
            <span class="timer-dot" />
            <strong>{{ formatTimer(elapsed) }}</strong>
            <button class="btn btn-ghost btn-sm" @click="stopTimer">
              <Square :size="13" />
              Stop
            </button>
          </div>
        </div>

        <div class="time-form">
          <div class="time-form-col">
            <label class="field">
              <span>Website (optional)</span>
              <select v-model="timerSiteId">
                <option value="">Ohne Website</option>
                <option v-for="site in detail.sites" :key="site.id" :value="site.id">
                  {{ site.label ?? site.url }}
                </option>
              </select>
            </label>

            <label class="field">
              <span>Tätigkeit</span>
              <input
                v-model="timerActivity"
                list="web-activities"
                type="text"
                placeholder="z. B. Plugin-Pflege"
              />
              <datalist id="web-activities">
                <option v-for="activity in webActivities" :key="activity" :value="activity" />
              </datalist>
            </label>
          </div>

          <label class="field time-form-comment">
            <span>Kommentar</span>
            <textarea
              v-model="timerComment"
              rows="5"
              placeholder="Was wurde gemacht? Für die Abrechnung."
            />
          </label>
        </div>

        <div class="time-form-foot">
          <p class="time-form-hint">
            <strong>Timer starten</strong> misst die Zeit live.
            <strong>Speichern</strong> trägt sie nachträglich mit der Dauer ein.
          </p>

          <label class="field field--dauer">
            <span>Dauer</span>
            <div class="dauer-input">
              <input v-model.number="manualMinutes" type="number" min="1" max="1440" step="5" />
              <span>min</span>
            </div>
          </label>

          <div class="time-form-buttons">
            <button class="btn btn-ghost" :disabled="savingManual" @click="saveManualEntry">
              <Save :size="15" />
              {{ savingManual ? 'Speichert …' : 'Speichern' }}
            </button>
            <button class="btn btn-primary" :disabled="isOurTimer" @click="startTimer">
              <Play :size="15" />
              {{ isOurTimer ? 'Timer läuft bereits' : 'Timer starten' }}
            </button>
          </div>
        </div>
      </section>

      <section v-if="runningAction" class="card action-progress">
        <div class="action-progress-head">
          <component :is="kindIcon(runningAction.kind)" :size="16" />
          <strong>{{ kindLabel(runningAction.kind) }}</strong>
          <span class="muted">{{ runningAction.status === 'running' ? 'läuft' : runningAction.status }}</span>
        </div>
        <div class="progress">
          <div
            class="progress-fill"
            :class="`tone-${actionTone(runningAction.status)}`"
            :style="{ width: `${Math.max(4, runningAction.progress)}%` }"
          />
        </div>
        <p class="action-progress-msg">{{ runningAction.message ?? 'Wird vorbereitet …' }}</p>
      </section>

      <div class="detail-grid">
        <section class="card">
          <header class="card-head">
            <div>
              <h2>Websites</h2>
              <p>Versionen, Updates und Zugangsdaten</p>
            </div>
            <div class="head-actions">
              <span class="count-pill">{{ detail.sites.length }}</span>
              <button class="btn btn-ghost btn-sm" @click="openSiteModal">
                <Plus :size="14" />
                Website hinzufügen
              </button>
            </div>
          </header>

          <div v-if="detail.sites.length" class="site-list">
            <article
              v-for="site in detail.sites"
              :key="site.id"
              class="site-card"
              :class="`site-card--${site.status}`"
            >
              <div class="site-head">
                <div class="site-title">
                  <div class="site-icon"><Globe2 :size="17" /></div>
                  <div>
                    <strong>{{ site.label ?? site.url }}</strong>
                    <a :href="site.url" target="_blank" rel="noreferrer">{{ site.url }}</a>
                  </div>
                </div>
                <StatusBadge :value="site.status" kind="site" />
              </div>

              <div class="site-meta">
                <span>WP {{ site.wp_version ?? '–' }}</span>
                <span>PHP {{ site.php_version ?? '–' }}</span>
                <span>{{ site.environment === 'prod' ? 'Produktiv' : site.environment === 'staging' ? 'Staging' : 'Lokal' }}</span>
                <span v-if="site.plugin_updates > 0" class="site-meta--warn">{{ site.plugin_updates }} Updates offen</span>
              </div>

              <div class="site-actions">
                <a class="btn btn-ghost btn-sm" :href="site.url" target="_blank" rel="noreferrer">
                  <ExternalLink :size="13" />
                  Öffnen
                </a>
                <a
                  v-if="site.login_url"
                  class="btn btn-ghost btn-sm"
                  :href="site.login_url"
                  target="_blank"
                  rel="noreferrer"
                >
                  <KeyRound :size="13" />
                  Login
                </a>
                <button
                  class="btn btn-ghost btn-sm"
                  :disabled="!site.agent_secret || actionBusy !== null"
                  @click="runAction(site.id, 'health')"
                >
                  <HeartPulse :size="13" />
                  Health
                </button>
                <button
                  class="btn btn-ghost btn-sm"
                  :disabled="!site.agent_secret || actionBusy !== null"
                  @click="runAction(site.id, 'plugins')"
                >
                  <Package :size="13" />
                  Plugins
                </button>
                <button
                  class="btn btn-ghost btn-sm"
                  :disabled="!site.agent_secret || actionBusy !== null"
                  @click="runAction(site.id, 'core')"
                >
                  <Upload :size="13" />
                  Core
                </button>
                <button class="btn btn-ghost btn-sm" @click="editSite(site)">
                  <Pencil :size="13" />
                  Bearbeiten
                </button>
                <button
                  class="btn btn-ghost btn-sm btn-danger-ghost"
                  :disabled="deletingSiteId === site.id"
                  @click="removeSite(site)"
                >
                  <Trash2 :size="13" />
                  {{ deletingSiteId === site.id ? 'Löscht …' : 'Löschen' }}
                </button>
              </div>

              <div v-if="site.login_user || site.login_password" class="cred-block">
                <div class="cred-row">
                  <span>Benutzer</span>
                  <code>{{ site.login_user ?? '–' }}</code>
                  <button class="icon-btn" title="Kopieren" @click="copy(site.login_user, 'Benutzer')">
                    <Copy :size="13" />
                  </button>
                </div>
                <div class="cred-row">
                  <span>Passwort</span>
                  <code>{{ revealPassword[site.id] ? site.login_password : '••••••••' }}</code>
                  <button
                    class="icon-btn"
                    :title="revealPassword[site.id] ? 'Verbergen' : 'Anzeigen'"
                    @click="togglePassword(site.id)"
                  >
                    <EyeOff v-if="revealPassword[site.id]" :size="13" />
                    <Eye v-else :size="13" />
                  </button>
                  <button class="icon-btn" title="Kopieren" @click="copy(site.login_password, 'Passwort')">
                    <Copy :size="13" />
                  </button>
                </div>
                <p v-if="site.login_note" class="cred-note">{{ site.login_note }}</p>
              </div>

              <p v-if="site.notes" class="site-note">{{ site.notes }}</p>
              <small class="site-checked">
                Zuletzt geprüft: {{ site.last_checked ? formatDateTime(site.last_checked) : 'nie' }}
              </small>
            </article>
          </div>

          <div v-else class="empty-state empty-state--compact">
            <Globe2 :size="22" />
            <p>Noch keine Website hinterlegt. Über „Website hinzufügen" Adresse und Zugang erfassen.</p>
          </div>
        </section>

        <section class="card">
          <header class="card-head">
            <div>
              <h2>Website-Vorschau</h2>
              <p>Klick öffnet die Seite und startet die Zeit</p>
            </div>
            <span class="count-pill">{{ detail.sites.length }}</span>
          </header>

          <div v-if="detail.sites.length" class="preview-stack">
            <article v-for="site in detail.sites" :key="`preview-${site.id}`" class="preview-card">
              <header class="preview-bar">
                <Globe :size="14" />
                <strong>{{ siteHost(site.url) }}</strong>
                <span class="preview-state">{{ site.status === 'healthy' ? 'stabil' : site.status }}</span>
                <button
                  class="preview-toggle"
                  :title="hiddenPreviews[site.id] ? 'Vorschau zeigen' : 'Vorschau ausblenden'"
                  @click="togglePreview(site)"
                >
                  <EyeOff v-if="!hiddenPreviews[site.id]" :size="13" />
                  <Eye v-else :size="13" />
                </button>
              </header>

              <button
                v-if="!hiddenPreviews[site.id]"
                class="preview-frame"
                :title="`${site.url} in neuem Tab öffnen und Zeit starten`"
                @click="openAndTrack(site)"
              >
                <iframe
                  :src="site.url"
                  :title="`Vorschau ${site.url}`"
                  class="preview-iframe"
                  loading="lazy"
                  referrerpolicy="no-referrer"
                  tabindex="-1"
                  aria-hidden="true"
                />
              </button>

              <p v-if="!hiddenPreviews[site.id]" class="preview-caption">
                <span>Klick öffnet die Seite in neuem Tab und startet die Zeit.</span>
                <small>Bleibt der Rahmen leer, verbietet die Seite das Einbetten.</small>
              </p>
            </article>
          </div>

          <div v-else class="empty-state empty-state--compact">
            <p>Noch keine Website hinterlegt.</p>
          </div>
        </section>
      </div>

      <div class="detail-grid">
        <section class="card">
          <header class="card-head">
            <div>
              <h2>Letzte Aktionen</h2>
              <p>Health, Plugin- und Core-Updates mit Nachweis</p>
            </div>
            <span class="count-pill">{{ detail.actions.length }}</span>
          </header>

          <div v-if="detail.actions.length" class="action-list">
            <div v-for="action in detail.actions" :key="action.id" class="action-row">
              <div class="action-icon" :class="`action-icon--${action.status}`">
                <component :is="kindIcon(action.kind)" :size="14" />
              </div>
              <div class="action-copy">
                <strong>{{ kindLabel(action.kind) }}</strong>
                <p>{{ action.message ?? '–' }}</p>
                <small>
                  {{ relativeTime(action.created_at) }}
                  <template v-if="action.duration_ms"> · {{ (action.duration_ms / 1000).toFixed(1) }} s</template>
                  <template v-if="action.after_version && action.before_version">
                    · {{ action.before_version }} → {{ action.after_version }}
                  </template>
                </small>
              </div>
              <span
                class="status-pill"
                :class="action.status === 'succeeded' ? 'status-pill--ok' : action.status === 'failed' ? 'status-pill--danger' : 'status-pill--warning'"
              >
                {{ action.status === 'succeeded' ? 'OK' : action.status === 'failed' ? 'Fehler' : 'läuft' }}
              </span>
            </div>
          </div>
          <div v-else class="empty-state empty-state--compact">
            <p>Noch keine Aktionen ausgeführt.</p>
          </div>
        </section>

      <section class="card">
        <header class="card-head">
          <div>
            <h2>Zeiten mit Kommentar</h2>
            <p>Kommentare sind für die Abrechnung gedacht – sie erscheinen im Monatsbericht.</p>
          </div>
          <span class="count-pill">{{ detail.time_entries.length }}</span>
        </header>

        <div v-if="detail.time_entries.length" class="table-wrap">
          <table class="data-table data-table--compact">
            <thead>
              <tr>
                <th>Datum</th>
                <th>Tätigkeit</th>
                <th>Website</th>
                <th class="num">Dauer</th>
                <th class="num">Betrag</th>
                <th>Kommentar</th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="entry in detail.time_entries" :key="entry.id">
                <td>
                  <div class="stacked-cell">
                    <span>{{ formatDate(entry.started_at) }}</span>
                    <small>{{ new Date(entry.started_at).toLocaleTimeString('de-DE', { hour: '2-digit', minute: '2-digit' }) }}</small>
                  </div>
                </td>
                <td>
                  <div class="stacked-cell">
                    <span>{{ entry.activity }}</span>
                    <small>{{ sourceLabel(entry.source) }}{{ entry.billable ? '' : ' · nicht abrechenbar' }}</small>
                  </div>
                </td>
                <td>{{ entry.site_label ?? '–' }}</td>
                <td class="num">{{ formatDuration(entry.duration_seconds) }}</td>
                <td class="num">
                  {{ entry.billable ? currency((entry.duration_seconds / 3600) * entry.hourly_rate) : '–' }}
                </td>
                <td class="comment-cell">
                  <input
                    v-model="commentDraft[entry.id]"
                    type="text"
                    class="comment-input"
                    placeholder="Kommentar ergänzen …"
                    :disabled="savingComment === entry.id"
                    @blur="saveComment(entry)"
                    @keyup.enter="($event.target as HTMLInputElement).blur()"
                  />
                </td>
              </tr>
            </tbody>
          </table>
        </div>
        <div v-else class="empty-state empty-state--compact">
          <p>Noch keine Zeiten erfasst. Starte oben einen Timer.</p>
        </div>
      </section>
      </div>
    </template>

    <Modal
      :open="commentModalOpen"
      title="Was wurde gemacht?"
      :subtitle="`Website geändert: ${commentContext}`"
      @close="commentModalOpen = false"
    >
      <label class="field">
        <span>Kommentar für die Abrechnung</span>
        <textarea
          v-model="modalComment"
          rows="4"
          autofocus
          placeholder="z. B. Plugin aktualisiert, Formular danach geprüft"
          @keydown.ctrl.enter="saveCommentModal"
          @keydown.meta.enter="saveCommentModal"
        />
      </label>

      <label v-if="!timerForThisClient" class="field field--dauer" style="margin-top: 12px">
        <span>Dauer</span>
        <div class="dauer-input">
          <input v-model.number="commentMinutes" type="number" min="1" max="1440" step="5" />
          <span>min</span>
        </div>
      </label>

      <p class="form-hint">
        Der Kommentar erscheint im Monatsbericht beim Kunden. Mit
        <kbd>Strg</kbd> + <kbd>Enter</kbd> speichern.
      </p>

      <template #footer>
        <button class="btn btn-ghost" type="button" @click="commentModalOpen = false">
          Ohne Kommentar
        </button>
        <button class="btn btn-primary" type="button" :disabled="commentSaving" @click="saveCommentModal">
          <MessageSquarePlus :size="15" />
          {{ commentSaving ? 'Speichert …' : 'Kommentar speichern' }}
        </button>
      </template>
    </Modal>

    <Modal
      :open="deleteClientOpen"
      title="Web-Kunde löschen"
      subtitle="Entfernt Stammdaten, Websites und Agent-Zugänge."
      @close="deleteClientOpen = false"
    >
      <div class="danger-block">
        <TriangleAlert :size="18" />
        <div>
          <strong>{{ detail?.client.name }}</strong>
          <p>
            Gelöscht werden der Kunde, seine {{ detail?.sites.length ?? 0 }} Websites, deren
            Zugangsdaten und alle Agent-Aktionen. Das lässt sich nicht rückgängig machen.
          </p>
        </div>
      </div>

      <p v-if="deleteClientError" class="form-error">{{ deleteClientError }}</p>

      <p v-else class="form-hint">
        Gibt es Zeiteinträge, verweigert das Cockpit das Löschen – dann bleibt die Abrechnung
        unangetastet. In dem Fall lässt sich der Kunde stattdessen deaktivieren.
      </p>

      <template #footer>
        <button class="btn btn-ghost" type="button" @click="deleteClientOpen = false">
          Abbrechen
        </button>
        <button
          class="btn btn-danger"
          type="button"
          :disabled="clientBusy"
          @click="deleteClient"
        >
          <Trash2 :size="15" />
          {{ clientBusy ? 'Löscht …' : 'Endgültig löschen' }}
        </button>
      </template>
    </Modal>

    <Modal
      :open="siteModalOpen"
      :title="editingSiteId ? 'Website bearbeiten' : 'Website hinzufügen'"
      :subtitle="
        editingSiteId
          ? 'Leere Passwortfelder lassen den gespeicherten Wert unverändert.'
          : 'Adresse, Agent-Zugang und – falls gewünscht – Login-Daten für den Schnellzugriff.'
      "
      @close="siteModalOpen = false"
    >
      <form class="form-grid" @submit.prevent="saveSite">
        <label class="field field--full">
          <span>Internetadresse</span>
          <input v-model="siteForm.url" type="text" placeholder="www.example.de" required />
        </label>

        <label class="field">
          <span>Bezeichnung</span>
          <input v-model="siteForm.label" type="text" placeholder="z. B. Hauptseite" />
        </label>

        <label class="field">
          <span>Umgebung</span>
          <select v-model="siteForm.environment">
            <option value="prod">Produktiv</option>
            <option value="staging">Staging</option>
            <option value="local">Lokal</option>
          </select>
        </label>

        <div class="form-section field--full">Agent (für Health und Updates)</div>

        <label class="field field--full">
          <span>Agent-Benutzer</span>
          <input v-model="siteForm.agent_user" type="text" placeholder="wartung-cockpit" />
        </label>

        <label class="field field--full">
          <span>Application Password</span>
          <input
            v-model="siteForm.agent_password"
            type="password"
            autocomplete="new-password"
            :placeholder="editingSiteId ? 'leer lassen = unverändert' : ''"
          />
        </label>

        <label class="field field--full">
          <span>Agent-Secret (WC_MAINT_SHARED_SECRET)</span>
          <input
            v-model="siteForm.agent_secret"
            type="password"
            autocomplete="new-password"
            :placeholder="editingSiteId ? 'leer lassen = unverändert' : ''"
          />
        </label>

        <div class="form-section field--full">Login für den Schnellzugriff (optional)</div>

        <label class="field field--full">
          <span>Login-Adresse</span>
          <input v-model="siteForm.login_url" type="text" placeholder="https://www.example.de/wp-login.php" />
        </label>

        <label class="field">
          <span>Login-Benutzer</span>
          <input v-model="siteForm.login_user" type="text" />
        </label>

        <label class="field">
          <span>Login-Passwort</span>
          <input
            v-model="siteForm.login_password"
            type="password"
            autocomplete="new-password"
            :placeholder="editingSiteId ? 'leer lassen = unverändert' : ''"
          />
        </label>

        <label class="field field--full">
          <span>Hinweis zum Login</span>
          <input v-model="siteForm.login_note" type="text" placeholder="z. B. Passwort im Passwortmanager" />
        </label>

        <label class="field field--full">
          <span>Notizen</span>
          <textarea v-model="siteForm.notes" rows="2" placeholder="Hosting, Besonderheiten, Backup-Weg …" />
        </label>

        <p class="form-hint field--full">
          Alle Zugangsdaten liegen im PoC unverschlüsselt in der Datenbank. Für echte Kundensites
          gehört eine Verschlüsselung mit Master-Key dazu. Beim Anmelden gilt: viele Seiten verbieten
          das Einbetten per iFrame, deshalb öffnet „Login" ein neues Fenster.
        </p>

        <p v-if="editingSiteId" class="form-hint field--full">
          Ein geleertes Textfeld wird gespeichert als „nicht gesetzt". Die drei Passwortfelder sind
          die Ausnahme: bleiben sie leer, gilt weiter das gespeicherte Passwort.
        </p>

        <p v-if="siteError" class="form-error field--full">{{ siteError }}</p>
      </form>

      <template #footer>
        <button class="btn btn-ghost" type="button" @click="siteModalOpen = false">Abbrechen</button>
        <button class="btn btn-primary" type="button" :disabled="siteSaving" @click="saveSite">
          {{
            siteSaving
              ? 'Speichert …'
              : editingSiteId
                ? 'Änderungen speichern'
                : 'Website anlegen'
          }}
        </button>
      </template>
    </Modal>
  </div>
</template>
