<script setup lang="ts">
import { computed, nextTick, onMounted, ref, watch } from 'vue'
import { ChevronLeft, ChevronRight, FileText, Printer, ShieldAlert, Wrench } from 'lucide-vue-next'
import { api, errorMessage } from '../api'
import type { MonthlyReport } from '../types'
import {
  currency,
  currentMonth,
  formatDate,
  formatDateTime,
  formatDuration,
  formatHours,
  monthLabel,
  shiftMonth,
} from '../format'

const month = ref(currentMonth())
const includeIdle = ref(false)
const report = ref<MonthlyReport | null>(null)
const loading = ref(true)
const error = ref('')

const blocks = computed(() => report.value?.blocks ?? [])
/** Kunden, für die der Standard-Satz greift, weil kein Vertragssatz gepflegt ist. */
const standardRate = computed(() =>
  blocks.value.filter((block) => block.rate_source === 'standard' && block.billable_hours > 0),
)

/** Aktuell gedruckter Block – `null` heißt: ganzer Bericht. */
const printTarget = ref<string | null>(null)

/// Ein Block ist ein Konto oder ein einzelner Kunde; der Schlüssel muss stabil
/// sein, sonst druckt „nur diesen Kunden" den falschen Bogen.
function blockKey(block: { account_id: string | null; customer_id: string }): string {
  return block.account_id ?? `source:${block.customer_id}`
}
const totals = computed(() => report.value?.totals ?? null)

/** Alphabetisch – der Bericht wird am Monatsende von oben nach unten abgearbeitet. */
const orderedBlocks = computed(() =>
  [...blocks.value].sort((a, b) => a.customer_name.localeCompare(b.customer_name, 'de')),
)

async function load() {
  loading.value = true
  error.value = ''
  try {
    report.value = await api.monthlyReport(month.value, includeIdle.value)
  } catch (caught) {
    error.value = errorMessage(caught)
  } finally {
    loading.value = false
  }
}

function changeMonth(delta: number) {
  month.value = shiftMonth(month.value, delta)
}

function printReport() {
  printTarget.value = null
  window.print()
}

/**
 * Druckt nur den Block eines Kunden. Der Filter greift erst, wenn Vue den
 * Rahmen neu gezeichnet hat – sonst würde der volle Bericht auf dem Papier landen.
 */
async function printBlock(key: string) {
  printTarget.value = key
  await nextTick()
  window.print()
  printTarget.value = null
}

function runStatus(status: string): string {
  return (
    {
      planned: 'geplant',
      in_progress: 'lief',
      done: 'abgeschlossen',
      blocked: 'blockiert',
    }[status] ?? status
  )
}

function taskStatus(status: string): string {
  return (
    {
      open: 'offen',
      running: 'lief',
      done: 'erledigt',
      failed: 'Fehler',
      skipped: 'übersprungen',
    }[status] ?? status
  )
}

/** Ansprechpartner, E-Mail und Vertrag als eine Zeile, ohne leere Trennzeichen. */
function metaLine(block: MonthlyReport['blocks'][number]): string {
  return [block.contact_name, block.contact_email, block.contract_name].filter(Boolean).join(' · ')
}

/** Datum mit Uhrzeit, kompakt – für die Zeilentabelle. */
function stamp(value: string): string {
  const date = new Date(value)
  if (Number.isNaN(date.getTime())) return value
  const day = date.toLocaleDateString('de-DE', { day: '2-digit', month: '2-digit' })
  const time = date.toLocaleTimeString('de-DE', { hour: '2-digit', minute: '2-digit' })
  return `${day.replace(/\.$/, '')}. ${time}`
}

function actionStatus(status: string): string {
  return (
    {
      succeeded: 'erfolgreich',
      failed: 'Fehler',
      running: 'lief',
      queued: 'angestoßen',
      planned: 'angestoßen',
      skipped: 'übersprungen',
    }[status] ?? status
  )
}

onMounted(load)
watch([month, includeIdle], load)
</script>

<template>
  <div class="page-stack report-page">
    <div class="page-toolbar page-toolbar--wrap no-print">
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
        <label class="checkbox">
          <input v-model="includeIdle" type="checkbox" />
          <span>Kunden ohne Aktivität zeigen</span>
        </label>
        <button class="btn btn-primary" :disabled="!report" @click="printReport">
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

    <article v-else-if="report" class="report-doc" :class="{ 'is-print-single': printTarget }">
      <header class="report-head">
        <div class="report-head-brand">
          <img src="/itsolutio-wordmark.png" alt="it.solutio" class="report-logo" />
          <p class="report-head-sub">Wartungscockpit · Leistungsnachweis</p>
        </div>
        <div class="report-head-meta">
          <p class="report-kicker">Leistungszeitraum</p>
          <h1>{{ monthLabel(report.month) }}</h1>
          <p v-if="totals">
            {{ totals.customers }} {{ totals.customers === 1 ? 'Kunde' : 'Kunden' }} ·
            {{ totals.runs }} {{ totals.runs === 1 ? 'Wartungslauf' : 'Wartungsläufe' }} ·
            {{ totals.actions }} {{ totals.actions === 1 ? 'Website-Aktion' : 'Website-Aktionen' }}
          </p>
        </div>
      </header>

      <section class="report-summary">
        <h2>Zusammenfassung</h2>
        <table class="report-table">
          <thead>
            <tr>
              <th>Kunde</th>
              <th>Vertrag</th>
              <th class="num">Abrechenbar</th>
              <th class="num">Überhang</th>
              <th class="num">Satz</th>
              <th class="num">Zeiten</th>
              <th class="num">Pauschale</th>
              <th class="num">Summe</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="block in orderedBlocks" :key="block.customer_id">
              <td>
                <strong>{{ block.customer_name }}</strong>
                <span class="report-code">{{ block.short_code }}</span>
              </td>
              <td>{{ block.contract_name ?? '–' }}</td>
              <td class="num">
                {{ formatDuration(block.billable_seconds) }}
                <span v-if="block.included_hours" class="report-included">
                  inkl. {{ formatHours(block.included_hours) }}
                </span>
              </td>
              <td class="num">
                {{ block.overage_seconds ? formatDuration(block.overage_seconds) : '–' }}
              </td>
              <td class="num">
                {{ block.hourly_rate ? currency(block.hourly_rate) : '–' }}
                <span v-if="block.rate_source === 'standard'" class="report-included">Standard</span>
              </td>
              <td class="num">{{ currency(block.time_amount) }}</td>
              <td class="num">{{ block.flat_fee ? currency(block.flat_fee) : '–' }}</td>
              <td class="num strong">{{ currency(block.total_amount) }}</td>
            </tr>
            <tr v-if="!orderedBlocks.length">
              <td colspan="8" class="report-empty">
                In {{ monthLabel(report.month) }} wurde für keinen Kunden Zeit erfasst.
              </td>
            </tr>
          </tbody>
          <tfoot v-if="totals && orderedBlocks.length">
            <tr>
              <td colspan="2">Gesamt</td>
              <td class="num">{{ formatDuration(totals.billable_seconds) }}</td>
              <td colspan="2" />
              <td class="num">{{ currency(totals.time_amount) }}</td>
              <td class="num">{{ currency(totals.flat_fees) }}</td>
              <td class="num strong">{{ currency(totals.total_amount) }}</td>
            </tr>
            <tr class="report-vat-row">
              <td colspan="6" />
              <td class="num">+ {{ totals.vat_rate.toLocaleString('de-DE') }} % USt</td>
              <td class="num">{{ currency(totals.vat_amount) }}</td>
            </tr>
            <tr class="report-vat-row">
              <td colspan="6" />
              <td class="num">Brutto</td>
              <td class="num strong">{{ currency(totals.gross_amount) }}</td>
            </tr>
          </tfoot>
        </table>
        <p v-if="standardRate.length" class="report-note">
          Ohne eigenen Vertragssatz gerechnet mit dem Standard-Satz
          {{ currency(standardRate[0].hourly_rate) }} netto:
          {{ standardRate.map((block) => block.customer_name).join(', ') }}.
        </p>
        <p class="report-note">
          Nicht abrechenbar in diesem Monat:
          {{ formatHours(totals?.non_billable_hours ?? 0) }} (interne Zeit, Maschinenzeit).
        </p>
        <p class="report-note">
          Abgerechnet wird die tatsächlich erfasste Zeit, sekundengenau – keine Mindestzeiten,
          keine Aufrundung. Betrag je Position = Dauer × Stundensatz, kaufmännisch auf Cent
          gerundet; die Summe ist die Addition dieser Positionen. Bei Verträgen mit
          Inklusivvolumen wird nur der Überhang darüber berechnet.
        </p>
      </section>

      <section
        v-for="block in orderedBlocks"
        :key="`detail-${block.customer_id}`"
        class="report-block"
        :class="{ 'is-print-target': printTarget === blockKey(block) }"
      >
        <header class="report-block-head">
          <div>
            <h2>
              {{ block.customer_name }}
              <span class="report-code">{{ block.short_code }}</span>
            </h2>
            <p v-if="metaLine(block)" class="report-block-meta">{{ metaLine(block) }}</p>
            <p v-if="block.members.length > 1" class="report-block-meta">
              Zusammengefasst: {{ block.members.join(' · ') }}
            </p>
            <p v-if="block.contract_conflict" class="report-note report-note--warn">
              In diesem Konto sind mehrere Verträge hinterlegt. Berechnet wurde nach
              „{{ block.contract_name }}“. Bitte die Zuordnung prüfen.
            </p>
          </div>
          <div class="report-block-amount">
            <strong>{{ currency(block.total_amount) }}</strong>
            <small>
              netto · {{ currency(block.gross_amount) }} brutto ·
              {{ formatDuration(block.billable_seconds) }} abrechenbar
            </small>
            <button class="report-block-print no-print" @click="printBlock(blockKey(block))">
              <Printer :size="13" />
              Nur diesen Kunden drucken
            </button>
          </div>
        </header>

        <header class="report-single-head">
          <img src="/itsolutio-wordmark.png" alt="it.solutio" class="report-logo" />
          <div>
            <p class="report-head-sub">Wartungscockpit · Leistungsnachweis</p>
            <h1>{{ block.customer_name }}</h1>
            <p class="report-block-period">{{ monthLabel(report.month) }}</p>
          </div>
        </header>

        <template v-if="block.runs.length">
          <h3 class="report-sub">
            <Wrench :size="14" />
            Durchgeführte Wartungen
          </h3>
          <div v-for="run in block.runs" :key="run.title + run.taken_at" class="report-run">
            <p class="report-run-head">
              <strong>{{ run.title }}</strong>
              <span>{{ formatDate(run.taken_at) }} · {{ runStatus(run.status) }}</span>
              <span v-if="run.assignee"> · {{ run.assignee }}</span>
            </p>
            <ul class="report-task-list">
              <li v-for="task in run.tasks" :key="task.title">
                <span class="report-task-status" :class="`is-${task.status}`">
                  {{ taskStatus(task.status) }}
                </span>
                <span>
                  {{ task.title }}
                  <template v-if="task.target"> · {{ task.target }}</template>
                </span>
                <span v-if="task.result" class="report-task-result">{{ task.result }}</span>
              </li>
            </ul>
          </div>
        </template>

        <template v-if="block.sites.length">
          <h3 class="report-sub">Websites</h3>
          <div v-for="site in block.sites" :key="site.url" class="report-run">
            <p class="report-run-head">
              <strong>{{ site.label }}</strong>
              <span>{{ site.url }}</span>
              <span v-if="site.wp_version"> · WordPress {{ site.wp_version }}</span>
              <span v-if="site.plugin_updates">
                · {{ site.plugin_updates }} offene Plugin-Updates
              </span>
            </p>
            <ul v-if="site.actions.length" class="report-task-list">
              <li v-for="action in site.actions" :key="action.label + action.taken_at">
                <span class="report-task-status" :class="`is-${action.status}`">
                  {{ actionStatus(action.status) }}
                </span>
                <span>
                  {{ action.label }} am {{ formatDateTime(action.taken_at) }}
                  <template v-if="action.before_version || action.after_version">
                    · {{ action.before_version ?? '?' }} → {{ action.after_version ?? '?' }}
                  </template>
                </span>
                <span v-if="action.message" class="report-task-result">{{ action.message }}</span>
              </li>
            </ul>
            <p v-else class="report-empty-line">Keine Aktion in diesem Monat.</p>
          </div>
        </template>

        <template v-if="block.entries.length">
          <h3 class="report-sub">
            <FileText :size="14" />
            Geleistete Zeit
          </h3>
          <table class="report-table report-table--times">
            <thead>
              <tr>
                <th>Datum</th>
                <th>Tätigkeit</th>
                <th>Kommentar</th>
                <th>Bezug</th>
                <th class="num">Dauer</th>
                <th class="num">Abrechenbar</th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="entry in block.entries" :key="entry.date + entry.activity + entry.duration_seconds">
                <td class="nowrap">{{ stamp(entry.date) }}</td>
                <td>
                  {{ entry.activity }}
                  <span v-if="block.members.length > 1 && entry.member" class="report-included">
                    {{ entry.member }}
                  </span>
                </td>
                <td class="report-comment">{{ entry.comment || '–' }}</td>
                <td>{{ entry.context ?? '–' }}</td>
                <td class="num nowrap">{{ formatDuration(entry.duration_seconds) }}</td>
                <td class="num nowrap">{{ entry.billable ? 'ja' : 'nein' }}</td>
              </tr>
            </tbody>
          </table>
        </template>

        <p v-if="!block.runs.length && !block.sites.length && !block.entries.length" class="report-empty-line">
          Keine Leistungen erfasst.
        </p>
      </section>

      <footer class="report-foot">
        <p>
          Erzeugt am {{ formatDateTime(report.generated_at) }}
          <template v-if="report.generated_by"> von {{ report.generated_by }}</template>
          · Wartungscockpit
        </p>
        <p class="report-signature">Unterschrift Auftragnehmer</p>
      </footer>
    </article>
  </div>
</template>
