<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { RouterLink, useRoute } from 'vue-router'
import {
  ArrowLeft,
  BadgeCheck,
  CheckCheck,
  FileText,
  Printer,
  ShieldAlert,
} from 'lucide-vue-next'
import { api, errorMessage } from '../api'
import type { Statement } from '../types'
import {
  currency,
  formatDate,
  formatDateTime,
  formatDuration,
  formatHours,
} from '../format'
import { useAppStore } from '../stores/app'

const route = useRoute()
const store = useAppStore()

const statement = ref<Statement | null>(null)
const loading = ref(true)
const error = ref('')
const marking = ref(false)
const markedMessage = ref('')

/** Web-Kunden und NinjaOne-Kunden teilen sich die Ansicht, nur der Aufruf unterscheidet sich. */
const isWeb = computed(() => route.name === 'web-statement')
const ownerId = computed(() => String(route.params.id))

const backTo = computed(() =>
  isWeb.value ? `/websites/${ownerId.value}` : `/kunden/${ownerId.value}`,
)

const billable = computed(() => statement.value?.entries.filter((entry) => entry.billable) ?? [])
const nonBillable = computed(
  () => statement.value?.entries.filter((entry) => !entry.billable) ?? [],
)

const period = computed(() => {
  const value = statement.value
  if (!value?.first_date || !value.last_date) return ''
  const first = formatDate(value.first_date)
  const last = formatDate(value.last_date)
  // Die Zeitstempel unterscheiden sich um die Uhrzeit; für die Anzeige
  // zählt nur, ob es derselbe Tag ist.
  return first === last ? first : `${first} – ${last}`
})

async function load() {
  loading.value = true
  error.value = ''
  markedMessage.value = ''
  try {
    statement.value = isWeb.value
      ? await api.webStatement(ownerId.value)
      : await api.statement(ownerId.value)
  } catch (caught) {
    error.value = errorMessage(caught)
  } finally {
    loading.value = false
  }
}

function print() {
  window.print()
}

/**
 * Setzt alle offenen Einträge der Aufstellung auf „abgerechnet". Die Auswahl
 * geht mit, damit ein parallel erfasster Eintrag nicht mit abgehakt wird.
 * Erst nach der Rückfrage: Danach taucht der Eintrag nicht mehr in der
 * Aufstellung auf.
 */
async function markBilled() {
  const value = statement.value
  if (!value || !value.entries.length) return

  const count = value.entries.length
  const confirmed = window.confirm(
    `${count} ${count === 1 ? 'Eintrag' : 'Einträge'} für ${value.name} als abgerechnet markieren?\n\n` +
      'Sie erscheinen danach nicht mehr in dieser Aufstellung. Die Zeiten selbst bleiben erhalten.',
  )
  if (!confirmed) return

  marking.value = true
  error.value = ''
  try {
    const ids = value.entries.map((entry) => entry.id)
    const result = isWeb.value
      ? await api.markWebStatementBilled(ownerId.value, ids)
      : await api.markStatementBilled(ownerId.value, ids)

    // Erst neu laden, dann melden: `load` setzt die Meldung zurück, sonst
    // wäre sie nach dem Aktualisieren der Liste sofort wieder weg.
    await load()
    markedMessage.value = `${result.marked} ${
      result.marked === 1 ? 'Eintrag' : 'Einträge'
    } als abgerechnet markiert.`
  } catch (caught) {
    error.value = errorMessage(caught)
  } finally {
    marking.value = false
  }
}

watch([isWeb, ownerId], load)
onMounted(load)
</script>

<template>
  <div class="page-stack statement-page">
    <div class="page-toolbar page-toolbar--wrap no-print">
      <RouterLink :to="backTo" class="btn btn-ghost">
        <ArrowLeft :size="16" />
        Zurück
      </RouterLink>

      <div class="toolbar-actions">
        <span v-if="markedMessage" class="status-pill status-pill--ok">
          <CheckCheck :size="13" />
          {{ markedMessage }}
        </span>
        <button
          v-if="store.canWrite && statement && statement.entries.length"
          class="btn btn-ghost"
          :disabled="marking"
          @click="markBilled"
        >
          <BadgeCheck :size="16" />
          {{ marking ? 'Wird markiert …' : 'Als abgerechnet markieren' }}
        </button>
        <button class="btn btn-primary" :disabled="!statement" @click="print">
          <Printer :size="16" />
          Drucken / als PDF speichern
        </button>
      </div>
    </div>

    <div v-if="loading" class="skeleton-stack">
      <div class="skeleton skeleton--panel" />
    </div>

    <div v-else-if="error" class="card empty-state">
      <ShieldAlert :size="26" />
      <p>{{ error }}</p>
      <button class="btn btn-primary" @click="load">Erneut versuchen</button>
    </div>

    <article v-else-if="statement" class="report-doc statement-doc">
      <header class="report-head">
        <div class="report-head-brand">
          <img src="/itsolutio-wordmark.png" alt="it.solutio" class="report-logo" />
          <p class="report-head-sub">Wartungscockpit · Aufstellung offener Leistungen</p>
        </div>
        <div class="report-head-meta">
          <p class="report-kicker">Leistungszeitraum</p>
          <h1>{{ period || 'keine offenen Zeiten' }}</h1>
          <p v-if="statement.contact_name || statement.contact_email">
            {{ [statement.contact_name, statement.contact_email].filter(Boolean).join(' · ') }}
          </p>
        </div>
      </header>

      <section class="statement-recipient">
        <p class="report-kicker">Für</p>
        <h2>
          {{ statement.name }}
          <span class="report-code">{{ statement.short_code }}</span>
        </h2>
        <p v-if="statement.contract_name" class="statement-contract">
          Vertrag: {{ statement.contract_name }}
        </p>
      </section>

      <template v-if="billable.length">
        <table class="report-table statement-table">
          <thead>
            <tr>
              <th>Datum</th>
              <th>Leistung</th>
              <th>Kommentar</th>
              <th>Bezug</th>
              <th>Person</th>
              <th class="num">Dauer</th>
              <th class="num">Betrag</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="entry in billable" :key="entry.id">
              <td class="nowrap">{{ formatDate(entry.date) }}</td>
              <td>{{ entry.activity }}</td>
              <td class="report-comment">{{ entry.comment || '–' }}</td>
              <td>{{ entry.context ?? '–' }}</td>
              <td>{{ entry.user_name }}</td>
              <td class="num nowrap">{{ formatDuration(entry.duration_seconds) }}</td>
              <td class="num nowrap">{{ currency(entry.amount) }}</td>
            </tr>
          </tbody>
          <tfoot>
            <tr>
              <td colspan="5">Summe abrechenbare Zeit</td>
              <td class="num nowrap">
                {{ formatDuration(statement.totals.billable_seconds) }}
              </td>
              <td class="num nowrap strong">{{ currency(statement.totals.time_amount) }}</td>
            </tr>
            <tr class="report-vat-row">
              <td colspan="5" />
              <td class="num">+ {{ statement.totals.vat_rate.toLocaleString('de-DE') }} % USt</td>
              <td class="num">{{ currency(statement.totals.vat_amount) }}</td>
            </tr>
            <tr class="report-vat-row">
              <td colspan="5" />
              <td class="num">Brutto</td>
              <td class="num strong">{{ currency(statement.totals.gross_amount) }}</td>
            </tr>
          </tfoot>
        </table>
      </template>

      <div v-else class="statement-empty">
        <FileText :size="22" />
        <p>
          Für {{ statement.name }} sind keine offenen, abrechenbaren Zeiten erfasst.
          Alles, was erfasst wurde, ist bereits abgerechnet.
        </p>
      </div>

      <p v-if="statement.entries.length && statement.rate_source === 'standard'" class="report-note">
        Im Vertrag ist kein eigener Stundensatz hinterlegt – gerechnet wurde mit dem
        Standard-Satz von {{ currency(statement.hourly_rate) }} netto.
      </p>
      <p v-else-if="statement.entries.length" class="report-note">
        Stundensatz aus dem Vertrag: {{ currency(statement.hourly_rate) }} netto.
      </p>
      <p v-if="statement.entries.length && statement.included_hours" class="report-note">
        Der Vertrag enthält {{ formatHours(statement.included_hours) }} je Monat. Diese
        Aufstellung weist den vollen Aufwand aus – der Abzug des Inklusivvolumens erfolgt
        auf der Rechnung.
      </p>
      <p v-if="nonBillable.length" class="report-note">
        Nicht ausgewiesen: {{ nonBillable.length }}
        {{ nonBillable.length === 1 ? 'Eintrag' : 'Einträge' }} mit
        {{ formatHours(statement.totals.non_billable_hours) }} nicht abrechenbarer Zeit.
        Sie werden beim Markieren trotzdem mit abgehakt.
      </p>
      <p class="report-note statement-basis">
        Abgerechnet wird die tatsächlich erfasste Zeit, sekundengenau: Betrag je Position =
        Dauer × Stundensatz, kaufmännisch auf Cent gerundet. Die Summe ist die Addition der
        Einzelbeträge. Keine Mindestzeiten, keine Aufrundung.
      </p>

      <footer class="report-foot">
        <p>
          Erzeugt am {{ formatDateTime(statement.generated_at) }}
          <template v-if="statement.generated_by"> von {{ statement.generated_by }}</template>
          · Wartungscockpit
        </p>
        <p class="report-signature">Unterschrift Auftragnehmer</p>
      </footer>
    </article>
  </div>
</template>
