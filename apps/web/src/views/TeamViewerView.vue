<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import {
  ChevronLeft,
  ChevronRight,
  Download,
  Link2,
  ShieldAlert,
  Undo2,
  X,
} from 'lucide-vue-next'
import { api, errorMessage } from '../api'
import Modal from '../components/Modal.vue'
import type {
  ActivityKind,
  CustomerSummary,
  TeamViewerGroup,
  TeamViewerSession,
  WebClientSummary,
} from '../types'
import {
  currency,
  currentMonth,
  formatDateTime,
  formatDurationCompact,
  monthLabel,
  shiftMonth,
} from '../format'

const month = ref(currentMonth())
const groups = ref<TeamViewerGroup[]>([])
const sessions = ref<TeamViewerSession[]>([])
const customers = ref<CustomerSummary[]>([])
const webClients = ref<WebClientSummary[]>([])
const activities = ref<ActivityKind[]>([])
const loading = ref(true)
const syncing = ref(false)
const error = ref('')
const notice = ref('')
const configured = ref<boolean | null>(null)
const lastSync = ref<string | null>(null)
const busyGroup = ref<string | null>(null)
const openGroup = ref<string | null>(null)

/**
 * Ausstehende Speicherungen.
 *
 * Wichtig wegen eines echten Fehlers: Wer die Dauer ändert und sofort
 * „Übernehmen“ drückt, war schneller als der Schreibvorgang – der Import las
 * dann noch die alte Zeit. Vor dem Übernehmen wird deshalb darauf gewartet.
 */
const pending = new Set<Promise<unknown>>()

function track<T>(work: Promise<T>): Promise<T> {
  const tracked = work.finally(() => {
    pending.delete(tracked as unknown as Promise<unknown>)
  })
  pending.add(tracked as unknown as Promise<unknown>)
  return tracked
}

async function flushPending() {
  if (!pending.size) return
  await Promise.allSettled([...pending])
}

/** Ziele für die Zuordnung: Kunden und Web-Kunden in einer Liste. */
const targets = computed(() => [
  ...customers.value.map((customer) => ({
    id: customer.id,
    label: customer.name,
    kind: 'customer' as const,
  })),
  ...webClients.value.map((client) => ({
    id: client.id,
    label: `${client.name} (Web)`,
    kind: 'web' as const,
  })),
])

const unassigned = computed(() =>
  groups.value.filter((group) => group.customer_id === null && group.web_client_id === null),
)

const summary = computed(() => ({
  seconds: sessions.value.reduce((sum, session) => sum + session.duration_seconds, 0),
  openSeconds: sessions.value
    .filter((session) => !session.imported_entry_id && !session.ignored)
    .reduce((sum, session) => sum + session.effective_seconds, 0),
  sessions: sessions.value.length,
  fee: sessions.value.reduce((sum, session) => sum + session.fee, 0),
}))

const shownSessions = computed(() =>
  openGroup.value ? sessions.value.filter((session) => session.group_id === openGroup.value) : [],
)

const currentGroup = computed(
  () => groups.value.find((group) => group.group_id === openGroup.value) ?? null,
)

const windowSeconds = computed(() =>
  shownSessions.value
    .filter((session) => !session.ignored && !session.imported_entry_id)
    .reduce((sum, session) => sum + session.effective_seconds, 0),
)

const windowCount = computed(
  () =>
    shownSessions.value.filter((session) => !session.ignored && !session.imported_entry_id).length,
)

async function load() {
  loading.value = true
  error.value = ''
  try {
    const [groupList, sessionList, customerList, clientList, status, activityList] =
      await Promise.all([
        api.teamviewerGroups(month.value),
        api.teamviewerSessions(month.value),
        api.customers(),
        api.webClients(),
        api.teamviewerStatus(),
        api.activities().catch(() => [] as ActivityKind[]),
      ])

    groups.value = groupList
    sessions.value = sessionList
    customers.value = customerList
    webClients.value = clientList
    activities.value = activityList
    configured.value = status.configured
    lastSync.value = status.last_sync
  } catch (caught) {
    error.value = errorMessage(caught)
  } finally {
    loading.value = false
  }
}

async function sync() {
  syncing.value = true
  error.value = ''
  notice.value = ''
  try {
    const report = await api.teamviewerSync(month.value)
    notice.value = report.message
    await load()
  } catch (caught) {
    error.value = errorMessage(caught)
  } finally {
    syncing.value = false
  }
}

function changeMonth(delta: number) {
  month.value = shiftMonth(month.value, delta)
}

/** Zuordnung setzen – der Wert ist `customer:…`, `web:…` oder leer. */
async function assign(group: TeamViewerGroup, value: string) {
  busyGroup.value = group.group_id
  error.value = ''
  notice.value = ''
  try {
    const [kind, id] = value.split(':')
    await api.teamviewerAssign(group.group_id, {
      customer_id: kind === 'customer' ? id : null,
      web_client_id: kind === 'web' ? id : null,
    })
    await load()
  } catch (caught) {
    error.value = errorMessage(caught)
  } finally {
    busyGroup.value = null
  }
}

async function hide(group: TeamViewerGroup) {
  busyGroup.value = group.group_id
  try {
    await api.teamviewerAssign(group.group_id, { ignored: true })
    await load()
  } catch (caught) {
    error.value = errorMessage(caught)
  } finally {
    busyGroup.value = null
  }
}

function groupValue(group: TeamViewerGroup): string {
  if (group.customer_id) return `customer:${group.customer_id}`
  if (group.web_client_id) return `web:${group.web_client_id}`
  return ''
}

/**
 * Verbindungen als Zeiteinträge übernehmen.
 *
 * Verbindungszeit ist nicht automatisch Arbeitszeit – deshalb eine bewusste
 * Rückfrage, und Sitzungen über vier Stunden werden vorher genannt.
 */
async function importSessions(group: TeamViewerGroup) {
  const count = group.open_sessions
  if (!count) return

  const long = sessions.value.filter(
    (session) =>
      session.group_id === group.group_id &&
      session.unusual &&
      !session.imported_entry_id &&
      !session.ignored,
  )

  let hint = ''
  if (long.length) {
    hint =
      `\n\nAchtung: ${long.length} Sitzung(en) dauern vier Stunden oder länger – ` +
      'oft ein vergessener Logout. Bitte vorher prüfen.'
  }

  if (
    !window.confirm(
      `${count} offene Verbindung(en) von „${group.name}“ als abrechenbare Zeiteinträge übernehmen?${hint}`,
    )
  ) {
    return
  }

  busyGroup.value = group.group_id
  error.value = ''
  notice.value = ''
  try {
    // Erst schreiben, dann importieren: Sonst übernimmt der Import die
    // Verbindungsdauer statt der gerade geänderten.
    await flushPending()
    const report = await api.teamviewerImport(group.group_id, month.value, true)
    notice.value = report.message
    await load()
  } catch (caught) {
    error.value = errorMessage(caught)
  } finally {
    busyGroup.value = null
  }
}

function toggleSessions(group: TeamViewerGroup) {
  openGroup.value = openGroup.value === group.group_id ? null : group.group_id
}

function closeSessions() {
  openGroup.value = null
}

/** Minuten für das Eingabefeld – auf ganze Minuten gerundet. */
function minutesOf(session: { effective_seconds: number }): number {
  return Math.round(session.effective_seconds / 60)
}

async function reloadSessions() {
  sessions.value = await api.teamviewerSessions(month.value)
}

/** Gruppenzahlen nachziehen, ohne die ganze Seite neu zu bauen. */
async function refreshGroups() {
  try {
    groups.value = await api.teamviewerGroups(month.value)
  } catch {
    // Die Sitzungsliste ist wichtiger; der nächste Abruf zieht die Zahlen nach.
  }
}

/**
 * Notiz speichern. Nur senden, wenn sich etwas geändert hat – sonst entstehen
 * bei jedem Verlassen des Feldes überflüssige Schreibvorgänge.
 */
function saveNote(session: TeamViewerSession, value: string) {
  const next = value.trim()
  if (next === (session.effective_note ?? '')) return Promise.resolve()

  return track(
    (async () => {
      try {
        await api.teamviewerEditSession(session.id, { note: next })
        await reloadSessions()
        await refreshGroups()
      } catch (caught) {
        error.value = errorMessage(caught)
        await reloadSessions()
      }
    })(),
  )
}

function resetNote(session: TeamViewerSession) {
  return track(
    (async () => {
      try {
        await api.teamviewerEditSession(session.id, { note: '' })
        await reloadSessions()
      } catch (caught) {
        error.value = errorMessage(caught)
      }
    })(),
  )
}

/**
 * Dauer in Minuten setzen. `0` heißt: zurück auf die Verbindungsdauer.
 * Die Originalzeit bleibt gespeichert und wird unter dem Feld gezeigt.
 */
function saveDuration(session: TeamViewerSession, raw: string) {
  const minutes = Number.parseInt(raw, 10)
  if (Number.isNaN(minutes) || minutes < 0) {
    void reloadSessions()
    return Promise.resolve()
  }

  const seconds = minutes * 60
  const current = session.duration_override_seconds
  if (current !== null && seconds === current) return Promise.resolve()
  if (current === null && seconds === session.duration_seconds) return Promise.resolve()

  return track(
    (async () => {
      try {
        await api.teamviewerEditSession(session.id, { duration_seconds: seconds })
        await reloadSessions()
        await refreshGroups()
      } catch (caught) {
        error.value = errorMessage(caught)
        await reloadSessions()
      }
    })(),
  )
}

function resetDuration(session: TeamViewerSession) {
  return track(
    (async () => {
      try {
        await api.teamviewerEditSession(session.id, { duration_seconds: 0 })
        await reloadSessions()
        await refreshGroups()
      } catch (caught) {
        error.value = errorMessage(caught)
      }
    })(),
  )
}

/** Leistungsart setzen. */
function saveActivity(session: TeamViewerSession, value: string) {
  if (value === (session.effective_activity ?? '')) return Promise.resolve()

  return track(
    (async () => {
      try {
        await api.teamviewerEditSession(session.id, { activity: value })
        await reloadSessions()
      } catch (caught) {
        error.value = errorMessage(caught)
        await reloadSessions()
      }
    })(),
  )
}

/** Sitzung ausblenden (Duplikat) oder zurückholen. */
function toggleIgnored(session: TeamViewerSession) {
  return track(
    (async () => {
      try {
        await api.teamviewerEditSession(session.id, { ignored: !session.ignored })
        await reloadSessions()
        await refreshGroups()
      } catch (caught) {
        error.value = errorMessage(caught)
      }
    })(),
  )
}

/**
 * Übernahme zurücknehmen – der Weg zurück, weil aus TeamViewer übernommene
 * Zeiten nicht gelöscht werden dürfen.
 */
async function undoImport(session: TeamViewerSession) {
  if (
    !window.confirm(
      'Übernahme zurücknehmen? Der Zeiteintrag wird entfernt, die Verbindung ist danach wieder bearbeitbar.',
    )
  ) {
    return
  }

  try {
    await api.teamviewerUndoImport(session.id)
    await reloadSessions()
    await refreshGroups()
  } catch (caught) {
    error.value = errorMessage(caught)
  }
}

watch(month, load)
onMounted(load)
</script>

<template>
  <div class="page-stack">
    <div class="page-toolbar page-toolbar--wrap">
      <div class="month-nav">
        <button class="icon-btn" aria-label="Vorheriger Monat" @click="changeMonth(-1)">
          <ChevronLeft :size="18" />
        </button>
        <strong>{{ monthLabel(month) }}</strong>
        <button class="icon-btn" aria-label="Nächster Monat" @click="changeMonth(1)">
          <ChevronRight :size="18" />
        </button>
      </div>
      <div class="toolbar-actions">
        <span v-if="lastSync" class="toolbar-meta">
          Zuletzt abgeglichen: {{ formatDateTime(lastSync) }}
        </span>
        <button class="btn btn-primary" :disabled="syncing" @click="sync">
          <Download :size="15" />
          {{ syncing ? 'Wird abgerufen …' : 'Jetzt abgleichen' }}
        </button>
      </div>
    </div>

    <div v-if="error" class="card empty-state">
      <ShieldAlert :size="24" />
      <p>{{ error }}</p>
      <button class="btn btn-primary" @click="load">Erneut versuchen</button>
    </div>

    <div v-else-if="notice" class="settings-message settings-message--ok">{{ notice }}</div>

    <section v-if="configured === false" class="card empty-state">
      <ShieldAlert :size="26" />
      <h2>Kein TeamViewer-Zugang hinterlegt</h2>
      <p>
        Der Script-Token wird unter <strong>Einstellungen → TeamViewer</strong> hinterlegt
        (nur Administratoren).
      </p>
    </section>

    <template v-else>
      <section class="metric-strip">
        <article class="metric-card">
          <p>Verbindungen {{ monthLabel(month) }}</p>
          <strong>{{ summary.sessions }}</strong>
          <small>{{ formatDurationCompact(summary.seconds) }} Verbindungszeit</small>
        </article>
        <article class="metric-card">
          <p>Noch offen</p>
          <strong>{{ formatDurationCompact(summary.openSeconds) }}</strong>
          <small>nicht als Zeiteintrag übernommen</small>
        </article>
        <article class="metric-card" :class="{ 'metric-card--accent': unassigned.length > 0 }">
          <p>Ohne Zuordnung</p>
          <strong>{{ unassigned.length }}</strong>
          <small>{{
            unassigned.length ? 'Gruppen brauchen einen Kunden' : 'alle Gruppen zugeordnet'
          }}</small>
        </article>
        <article class="metric-card">
          <p>TeamViewer rechnet</p>
          <strong>{{ currency(summary.fee) }}</strong>
          <small>eigene Berechnung im TeamViewer-Konto</small>
        </article>
      </section>

      <section class="card">
        <header class="card-head">
          <div>
            <h2>Kundengruppen</h2>
            <p>
              Klick auf den Namen zeigt die Verbindungen. Zuordnen lässt sich auch gesammelt unter
              <strong>Einstellungen → Konten</strong>.
            </p>
          </div>
          <span class="count-pill">{{ groups.length }}</span>
        </header>

        <div v-if="loading" class="skeleton skeleton--panel" />

        <div v-else-if="!groups.length" class="empty-state empty-state--compact">
          <p>
            Für {{ monthLabel(month) }} sind keine Verbindungen abgelegt. Mit
            <strong>Jetzt abgleichen</strong> werden sie von TeamViewer geholt.
          </p>
        </div>

        <div v-else class="table-wrap">
          <table class="data-table tv-table">
            <thead>
              <tr>
                <th>TeamViewer-Gruppe</th>
                <th>Kunde</th>
                <th class="num">Verbindungen</th>
                <th class="num">Zeit</th>
                <th class="num">Offen</th>
                <th aria-label="Aktionen" />
              </tr>
            </thead>
            <tbody>
              <tr v-for="group in groups" :key="group.group_id">
                <td>
                  <button
                    class="link-button"
                    :title="`${group.sessions} Sitzungen anzeigen`"
                    @click="toggleSessions(group)"
                  >
                    {{ group.name }}
                    <ChevronRight :size="13" class="tv-chevron" />
                  </button>
                  <small class="tv-account">
                    {{ group.sessions }} Sitzungen ansehen
                    <template v-if="group.account_code"> · Konto {{ group.account_code }}</template>
                  </small>
                </td>
                <td>
                  <select
                    class="tv-select"
                    :value="groupValue(group)"
                    :disabled="busyGroup === group.group_id"
                    @change="assign(group, ($event.target as HTMLSelectElement).value)"
                  >
                    <option value="">
                      {{
                        group.suggested_name
                          ? `– Vorschlag: ${group.suggested_name} –`
                          : '– nicht zugeordnet –'
                      }}
                    </option>
                    <option
                      v-for="target in targets"
                      :key="target.id"
                      :value="`${target.kind}:${target.id}`"
                    >
                      {{ target.label }}
                    </option>
                  </select>
                </td>
                <td class="num">{{ group.sessions }}</td>
                <td class="num nowrap">{{ formatDurationCompact(group.seconds) }}</td>
                <td class="num nowrap">
                  <span v-if="group.open_sessions" class="status-pill status-pill--warning">
                    {{ formatDurationCompact(group.open_seconds) }}
                  </span>
                  <span v-else class="status-pill status-pill--ok">übernommen</span>
                </td>
                <td class="row-actions">
                  <button
                    class="btn btn-ghost btn--sm"
                    :disabled="
                      busyGroup === group.group_id ||
                      !group.open_sessions ||
                      (!group.customer_id && !group.web_client_id)
                    "
                    :title="
                      !group.customer_id && !group.web_client_id
                        ? 'Erst einem Kunden zuordnen'
                        : 'Offene Verbindungen als Zeiteinträge übernehmen'
                    "
                    @click="importSessions(group)"
                  >
                    <Link2 :size="14" />
                    Übernehmen
                  </button>
                  <button class="icon-btn" aria-label="Gruppe ausblenden" @click="hide(group)">
                    <X :size="15" />
                  </button>
                </td>
              </tr>
            </tbody>
          </table>
        </div>
      </section>
    </template>

    <Modal
      :open="openGroup !== null"
      size="wide"
      :title="currentGroup?.name ?? 'Verbindungen'"
      :subtitle="`${shownSessions.length} Sitzungen in ${monthLabel(month)}`"
      @close="closeSessions"
    >
      <p class="tv-modal-hint">
        Dauer, Leistungsart und Kommentar lassen sich anpassen – die Originalzeit von TeamViewer
        bleibt erhalten. Das <strong>×</strong> blendet eine Sitzung aus, etwa wenn ein
        Arbeitsvorgang auf zwei Rechnern protokolliert wurde.
      </p>

      <div v-if="!shownSessions.length" class="empty-state empty-state--compact">
        <p>Keine Verbindungen in diesem Monat.</p>
      </div>

      <div v-else class="tv-sessions">
        <article
          v-for="session in shownSessions"
          :key="session.id"
          class="tv-session"
          :class="{ 'is-ignored': session.ignored, 'is-imported': session.imported_entry_id }"
        >
          <header class="tv-session-head">
            <div class="tv-session-meta">
              <strong>{{ formatDateTime(session.started_at) }}</strong>
              <span class="tv-session-device">{{
                session.device_name ?? 'ohne Gerätenamen'
              }}</span>
              <span class="tv-session-user">{{ session.user_name ?? '–' }}</span>
            </div>
            <div class="tv-session-badges">
              <span v-if="session.imported_entry_id" class="status-pill status-pill--ok">
                übernommen
              </span>
              <span v-else-if="session.ignored" class="status-pill status-pill--muted">
                ausgeblendet
              </span>
              <span v-else-if="session.unusual" class="status-pill status-pill--warning">
                auffällig lang
              </span>
              <span v-else class="status-pill status-pill--muted">offen</span>

              <button
                class="icon-btn"
                :aria-label="session.ignored ? 'Sitzung zurückholen' : 'Sitzung ausblenden'"
                :title="
                  session.imported_entry_id
                    ? 'Bereits übernommen'
                    : session.ignored
                      ? 'Sitzung zurückholen'
                      : 'Sitzung ausblenden (Duplikat)'
                "
                :disabled="!!session.imported_entry_id"
                @click="toggleIgnored(session)"
              >
                <Undo2 v-if="session.ignored" :size="15" />
                <X v-else :size="15" />
              </button>
            </div>
          </header>

          <div class="tv-session-fields">
            <label class="field tv-duration">
              <span>Dauer (Minuten)</span>
              <input
                type="number"
                min="0"
                max="1440"
                step="1"
                :value="minutesOf(session)"
                :disabled="!!session.imported_entry_id || session.ignored"
                @change="saveDuration(session, ($event.target as HTMLInputElement).value)"
              />
              <small v-if="session.duration_override_seconds !== null" class="field-hint">
                Original {{ formatDurationCompact(session.duration_seconds) }}
                <button type="button" class="link-inline" @click="resetDuration(session)">
                  zurücksetzen
                </button>
              </small>
            </label>

            <label class="field tv-activity">
              <span>Leistungsart</span>
              <select
                :value="session.effective_activity"
                :disabled="!!session.imported_entry_id || session.ignored"
                @change="saveActivity(session, ($event.target as HTMLSelectElement).value)"
              >
                <option v-for="kind in activities" :key="kind.id" :value="kind.name">
                  {{ kind.name }}
                </option>
              </select>
            </label>
          </div>

          <label class="field tv-note">
            <span>Kommentar</span>
            <textarea
              rows="3"
              :value="session.effective_note ?? ''"
              :placeholder="session.notes ?? 'Kommentar …'"
              :disabled="!!session.imported_entry_id || session.ignored"
              @blur="saveNote(session, ($event.target as HTMLTextAreaElement).value)"
            />
            <small v-if="session.note_edit !== null" class="field-hint">
              TeamViewer-Original: {{ session.notes || '–' }}
              <button type="button" class="link-inline" @click="resetNote(session)">
                zurücksetzen
              </button>
            </small>
            <small v-else-if="session.notes" class="field-hint">
              Vorlage aus TeamViewer – überschreiben oder ergänzen
            </small>
          </label>

          <p v-if="session.imported_entry_id" class="tv-session-imported">
            Als Zeiteintrag übernommen.
            <button type="button" class="link-inline" @click="undoImport(session)">
              Übernahme zurücknehmen
            </button>
          </p>
        </article>
      </div>

      <template #footer>
        <div class="tv-modal-foot">
          <span class="tv-modal-sum">
            {{ windowCount }} offen · {{ formatDurationCompact(windowSeconds) }}
          </span>
          <button class="btn btn-ghost" @click="closeSessions">Schließen</button>
          <button
            class="btn btn-primary"
            :disabled="
              !currentGroup ||
              busyGroup === currentGroup.group_id ||
              !currentGroup.open_sessions ||
              (!currentGroup.customer_id && !currentGroup.web_client_id)
            "
            :title="
              !currentGroup?.customer_id && !currentGroup?.web_client_id
                ? 'Erst einem Kunden zuordnen'
                : 'Offene Verbindungen als Zeiteinträge übernehmen'
            "
            @click="currentGroup && importSessions(currentGroup)"
          >
            <Link2 :size="15" />
            Übernehmen
          </button>
        </div>
      </template>
    </Modal>
  </div>
</template>
