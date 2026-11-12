<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue'
import {
  ChevronLeft,
  ChevronRight,
  Clock3,
  Download,
  Euro,
  Plus,
  ReceiptText,
  ShieldAlert,
  Trash2,
  Users,
} from 'lucide-vue-next'
import { api, errorMessage } from '../api'
import type { ActiveTimer, BillingSummary, CustomerSummary, ManualTimePayload, TimeEntry } from '../types'
import {
  currency,
  currentMonth,
  formatDate,
  formatDuration,
  formatHours,
  monthLabel,
  shiftMonth,
} from '../format'
import Modal from '../components/Modal.vue'
import StatusBadge from '../components/StatusBadge.vue'
import { useAppStore } from '../stores/app'

const store = useAppStore()
const month = ref(currentMonth())
const entries = ref<TimeEntry[]>([])
const billing = ref<BillingSummary | null>(null)
const customers = ref<CustomerSummary[]>([])
const loading = ref(true)
const error = ref('')
const modalOpen = ref(false)
const saving = ref(false)
const formError = ref('')

const form = ref<ManualTimePayload>({
  customer_id: '',
  activity: '',
  note: '',
  date: new Date().toISOString().slice(0, 10),
  minutes: 30,
  billable: true,
})

const billableSeconds = computed(() =>
  entries.value.filter((entry) => entry.billable).reduce((sum, entry) => sum + entry.duration_seconds, 0),
)
const nonBillableSeconds = computed(() =>
  entries.value.filter((entry) => !entry.billable).reduce((sum, entry) => sum + entry.duration_seconds, 0),
)
const maxHours = computed(() =>
  Math.max(1, ...(billing.value?.customers.map((customer) => customer.billable_hours) ?? [1])),
)
const sortedBilling = computed(() =>
  [...(billing.value?.customers ?? [])].sort((a, b) => b.billable_hours - a.billable_hours),
)

const commentDraft = ref<Record<string, string>>({})
const savingComment = ref<string | null>(null)

/// Laufende Timer aller Personen – in einer Gruppe ist „wer arbeitet woran"
/// die Frage, die man sich zehnmal am Tag stellt.
const running = ref<ActiveTimer[]>([])
const othersRunning = computed(() =>
  running.value.filter((timer) => timer.user_name !== store.user?.display_name),
)

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

async function load() {
  loading.value = true
  error.value = ''
  try {
    const [entryList, summary, runningList] = await Promise.all([
      api.timeEntries(month.value),
      api.billing(month.value),
      api.runningTimers(),
    ])
    entries.value = entryList
    billing.value = summary
    running.value = runningList
    for (const entry of entryList) {
      commentDraft.value[entry.id] = entry.note ?? ''
    }
  } catch (caught) {
    error.value = errorMessage(caught)
  } finally {
    loading.value = false
  }
}

function changeMonth(delta: number) {
  month.value = shiftMonth(month.value, delta)
  load()
}

function exportCsv() {
  api.exportCsv(month.value)
  store.toast('CSV-Export wird geöffnet.', 'info')
}

function openModal() {
  form.value = {
    customer_id: customers.value[0]?.id ?? '',
    activity: '',
    note: '',
    date: new Date().toISOString().slice(0, 10),
    minutes: 30,
    billable: true,
  }
  formError.value = ''
  modalOpen.value = true
}

async function save() {
  if (!form.value.customer_id || !form.value.activity.trim() || form.value.minutes < 1) {
    formError.value = 'Bitte Kunde, Leistungsart und eine Dauer ab 1 Minute angeben.'
    return
  }

  saving.value = true
  formError.value = ''
  try {
    await api.createTimeEntry(form.value)
    modalOpen.value = false
    store.toast('Zeit erfasst.', 'success')
    await load()
  } catch (caught) {
    formError.value = errorMessage(caught)
  } finally {
    saving.value = false
  }
}

async function remove(entry: TimeEntry) {
  // Aus TeamViewer übernommene Zeiten sind ein Nachweis: Sie spiegeln eine
  // protokollierte Verbindung. Rücknahme läuft über den TeamViewer-Reiter.
  if (entry.source === 'teamviewer') {
    store.toast(
      'Dieser Eintrag stammt aus einer TeamViewer-Verbindung. Die Übernahme lässt sich ' +
        'unter TeamViewer zurücknehmen.',
      'error',
    )
    return
  }

  if (!window.confirm(`Eintrag „${entry.activity}“ wirklich löschen?`)) return
  try {
    await api.deleteTimeEntry(entry.id)
    store.toast('Eintrag gelöscht.', 'success')
    await load()
  } catch (caught) {
    store.toast(errorMessage(caught), 'error')
  }
}

async function refreshRunning() {
  try {
    running.value = await api.runningTimers()
  } catch {
    // Die Übersicht ist Beiwerk; ein Fehler darf die Zeiterfassung nicht stören.
  }
}

let runningPoller: number | undefined

onMounted(async () => {
  try {
    customers.value = await api.customers()
  } catch {
    // Die Zeiterfassung funktioniert auch ohne Kundenliste; das Formular zeigt dann einen Hinweis.
  }
  await load()
  runningPoller = window.setInterval(refreshRunning, 30_000)
})

onUnmounted(() => {
  if (runningPoller) window.clearInterval(runningPoller)
})
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
        <button class="btn btn-ghost" @click="exportCsv">
          <Download :size="16" />
          CSV exportieren
        </button>
        <button class="btn btn-primary" @click="openModal">
          <Plus :size="16" />
          Zeit erfassen
        </button>
      </div>
    </div>

    <section v-if="othersRunning.length" class="team-running">
      <div class="team-running-head">
        <Users :size="15" />
        <strong>Gerade in Arbeit</strong>
      </div>
      <div class="team-running-list">
        <span v-for="timer in othersRunning" :key="timer.id" class="team-running-item">
          <strong>{{ timer.user_name }}</strong>
          {{ timer.activity }} · {{ timer.customer_name }}
        </span>
      </div>
    </section>

    <div v-if="loading" class="skeleton-stack">
      <div class="kpi-grid">
        <div v-for="index in 4" :key="index" class="skeleton skeleton--kpi" />
      </div>
      <div class="skeleton skeleton--panel" />
    </div>

    <div v-else-if="error" class="card empty-state">
      <ShieldAlert :size="26" />
      <p>{{ error }}</p>
      <button class="btn btn-primary" @click="load">Erneut versuchen</button>
    </div>

    <template v-else>
      <section class="kpi-grid kpi-grid--four">
        <article class="kpi-card">
          <div class="kpi-icon kpi-icon--teal"><Clock3 :size="20" /></div>
          <div>
            <p class="kpi-label">Abrechenbar</p>
            <strong class="kpi-value">{{ formatDuration(billableSeconds) }}</strong>
            <p class="kpi-hint">in {{ monthLabel(month) }}</p>
          </div>
        </article>
        <article class="kpi-card">
          <div class="kpi-icon kpi-icon--amber"><Clock3 :size="20" /></div>
          <div>
            <p class="kpi-label">Nicht abrechenbar</p>
            <strong class="kpi-value">{{ formatDuration(nonBillableSeconds) }}</strong>
            <p class="kpi-hint">Doku und Maschinenzeit</p>
          </div>
        </article>
        <article class="kpi-card">
          <div class="kpi-icon kpi-icon--violet"><Euro :size="20" /></div>
          <div>
            <p class="kpi-label">Abrechnungssumme</p>
            <strong class="kpi-value">{{ billing ? currency(billing.total_amount) : '–' }}</strong>
            <p class="kpi-hint">inkl. Pauschalen</p>
          </div>
        </article>
        <article class="kpi-card">
          <div class="kpi-icon kpi-icon--blue"><ReceiptText :size="20" /></div>
          <div>
            <p class="kpi-label">Einträge</p>
            <strong class="kpi-value">{{ entries.length }}</strong>
            <p class="kpi-hint">{{ billing?.customers.length ?? 0 }} Kunden</p>
          </div>
        </article>
      </section>

      <div class="time-grid">
        <section class="card">
          <header class="card-head">
            <div>
              <h2>Stunden nach Kunde</h2>
              <p>Abrechenbare Leistung im Monat</p>
            </div>
          </header>
          <div class="bar-chart bar-chart--scroll">
            <div v-for="customer in sortedBilling" :key="customer.customer_id" class="bar-row">
              <span class="bar-label">{{ customer.customer_name }}</span>
              <div class="bar-track">
                <div
                  class="bar-fill"
                  :style="{ width: `${Math.max(3, (customer.billable_hours / maxHours) * 100)}%` }"
                />
              </div>
              <strong class="bar-value">{{ formatHours(customer.billable_hours) }}</strong>
            </div>
          </div>
        </section>

        <section class="card">
          <header class="card-head">
            <div>
              <h2>Vertragslogik</h2>
              <p>Inklusivstunden, Überhang und Pauschale</p>
            </div>
          </header>
          <div class="table-wrap table-scroll--capped">
            <table class="data-table data-table--compact">
              <thead>
                <tr>
                  <th>Kunde</th>
                  <th class="num">Leistung</th>
                  <th class="num">Inklusive</th>
                  <th class="num">Überhang</th>
                  <th class="num">Summe</th>
                </tr>
              </thead>
              <tbody>
                <tr v-for="customer in sortedBilling" :key="customer.customer_id">
                  <td>{{ customer.short_code }}</td>
                  <td class="num">{{ formatDuration(customer.billable_seconds) }}</td>
                  <td class="num">{{ formatHours(customer.included_hours) }}</td>
                  <td class="num">
                    <span :class="{ 'num-strong': customer.overage_seconds > 0 }">
                      {{ customer.overage_seconds ? formatDuration(customer.overage_seconds) : '–' }}
                    </span>
                  </td>
                  <td class="num">{{ currency(customer.total_amount) }}</td>
                </tr>
              </tbody>
            </table>
          </div>
        </section>
      </div>

      <section class="card">
        <header class="card-head">
          <div>
            <h2>Zeiteinträge</h2>
            <p>Timer, manuelle Einträge und automatische Maschinenzeit</p>
          </div>
          <span class="count-pill">{{ entries.length }}</span>
        </header>

        <div v-if="entries.length" class="table-wrap">
          <table class="data-table">
            <thead>
              <tr>
                <th>Datum</th>
                <th>Kunde</th>
                <th>Leistung</th>
                <th>Quelle</th>
                <th class="num">Dauer</th>
                <th class="num">Betrag</th>
                <th>Kommentar</th>
                <th>Abrechenbar</th>
                <th aria-label="Löschen" />
              </tr>
            </thead>
            <tbody>
              <tr v-for="entry in entries" :key="entry.id">
                <td>
                  <div class="stacked-cell">
                    <span>{{ formatDate(entry.started_at) }}</span>
                    <small v-if="entry.billed_at">abgerechnet</small>
                  </div>
                </td>
                <td>
                  <div class="stacked-cell">
                    <span>{{ entry.owner_short_code }}</span>
                    <small>{{ entry.owner_name }}</small>
                  </div>
                </td>
                <td>
                  <div class="stacked-cell">
                    <span>{{ entry.activity }}</span>
                    <small>{{ entry.device_label ?? entry.site_label ?? entry.task_title ?? '–' }}</small>
                  </div>
                </td>
                <td><StatusBadge :value="entry.source" kind="source" /></td>
                <td class="num">{{ formatDuration(entry.duration_seconds) }}</td>
                <td class="num">
                  {{ entry.billable ? currency(entry.amount) : '–' }}
                </td>
                <td class="comment-cell">
                  <input
                    v-model="commentDraft[entry.id]"
                    type="text"
                    class="comment-input"
                    placeholder="Kommentar …"
                    :disabled="savingComment === entry.id"
                    @blur="saveComment(entry)"
                    @keyup.enter="($event.target as HTMLInputElement).blur()"
                  />
                </td>
                <td>
                  <span class="status-pill" :class="entry.billable ? 'status-pill--ok' : 'status-pill--muted'">
                    {{ entry.billable ? 'ja' : 'nein' }}
                  </span>
                </td>
                <td class="chevron-cell">
                  <button
                    class="icon-btn icon-btn--danger"
                    :aria-label="
                      entry.source === 'teamviewer'
                        ? 'Aus TeamViewer übernommen – nicht löschbar'
                        : 'Eintrag löschen'
                    "
                    :title="
                      entry.source === 'teamviewer'
                        ? 'Aus TeamViewer übernommen – Rücknahme im TeamViewer-Reiter'
                        : 'Eintrag löschen'
                    "
                    :disabled="entry.source === 'teamviewer'"
                    @click="remove(entry)"
                  >
                    <Trash2 :size="15" />
                  </button>
                </td>
              </tr>
            </tbody>
          </table>
        </div>
        <div v-else class="empty-state empty-state--compact">
          <Clock3 :size="24" />
          <p>Für {{ monthLabel(month) }} sind noch keine Zeiten erfasst.</p>
        </div>
      </section>

      <section class="info-strip">
        <div class="info-strip-icon"><ReceiptText :size="18" /></div>
        <p>
          <strong>Export für die Abrechnung.</strong>
          Die CSV enthält Leistungsdatum, Kunde, Leistungsart, Menge, Einzelpreis, Betrag und
          Steuersatz. Die DATEV-Spalten werden vor dem Produktivbetrieb mit dem Steuerberater
          abgestimmt.
        </p>
      </section>
    </template>

    <Modal
      :open="modalOpen"
      title="Zeit manuell erfassen"
      subtitle="Für Arbeiten, die nicht über den Timer gelaufen sind."
      @close="modalOpen = false"
    >
      <form class="form-grid" @submit.prevent="save">
        <label class="field field--full">
          <span>Kunde</span>
          <select v-model="form.customer_id" required>
            <option v-for="customer in customers" :key="customer.id" :value="customer.id">
              {{ customer.name }}
            </option>
          </select>
        </label>

        <label class="field field--full">
          <span>Leistungsart</span>
          <input v-model="form.activity" type="text" placeholder="z. B. Patchmanagement" required />
        </label>

        <label class="field">
          <span>Datum</span>
          <input v-model="form.date" type="date" />
        </label>

        <label class="field">
          <span>Dauer (Minuten)</span>
          <input v-model.number="form.minutes" type="number" min="1" max="1440" step="5" required />
        </label>

        <label class="field field--full">
          <span>Beschreibung</span>
          <textarea v-model="form.note" rows="3" placeholder="Was wurde gemacht?" />
        </label>

        <label class="checkbox field--full">
          <input v-model="form.billable" type="checkbox" />
          <span>Abrechenbar</span>
        </label>

        <p v-if="formError" class="form-error field--full">{{ formError }}</p>
      </form>

      <template #footer>
        <button class="btn btn-ghost" type="button" @click="modalOpen = false">Abbrechen</button>
        <button class="btn btn-primary" type="button" :disabled="saving" @click="save">
          {{ saving ? 'Speichert …' : 'Zeit speichern' }}
        </button>
      </template>
    </Modal>
  </div>
</template>
