<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useRouter } from 'vue-router'
import {
  ArrowUpRight,
  CalendarClock,
  ExternalLink,
  CheckCircle2,
  Inbox,
  Plus,
  RefreshCw,
  ShieldAlert,
  Timer,
} from 'lucide-vue-next'
import { api, errorMessage } from '../api'
import type { CreateTicketPayload, CustomerSummary, Priority, RunListItem } from '../types'
import { ageDays, formatDate, formatDateTime, priorityLabel } from '../format'
import { TIX_URL } from '../links'
import Modal from '../components/Modal.vue'
import { useAppStore } from '../stores/app'

const router = useRouter()
const store = useAppStore()
const tickets = ref<RunListItem[]>([])
const customers = ref<CustomerSummary[]>([])
const loading = ref(true)
const error = ref('')
const filter = ref<'open' | 'done' | 'all'>('open')
const modalOpen = ref(false)
const saving = ref(false)
const formError = ref('')

const form = ref<CreateTicketPayload>({
  customer_id: '',
  device_id: '',
  title: '',
  description: '',
  priority: 'normal',
  channel: 'Telefon',
  assignee: '',
  due_at: null,
})

const priorities: Priority[] = ['urgent', 'high', 'normal', 'low']
const channels = ['Telefon', 'E-Mail', 'Vor Ort', 'Fernzugriff', 'Monitoring', 'Sonstiges']

const visible = computed(() => {
  if (filter.value === 'open') return tickets.value.filter((ticket) => ticket.status !== 'done')
  if (filter.value === 'done') return tickets.value.filter((ticket) => ticket.status === 'done')
  return tickets.value
})

const counts = computed(() => ({
  open: tickets.value.filter((ticket) => ticket.status !== 'done').length,
  done: tickets.value.filter((ticket) => ticket.status === 'done').length,
  urgent: tickets.value.filter(
    (ticket) => ticket.status !== 'done' && (ticket.priority === 'urgent' || ticket.priority === 'high'),
  ).length,
}))

async function load() {
  loading.value = true
  error.value = ''
  try {
    const [ticketList, customerList] = await Promise.all([api.tickets(), api.customers()])
    tickets.value = ticketList
    customers.value = customerList
  } catch (caught) {
    error.value = errorMessage(caught)
  } finally {
    loading.value = false
  }
}

function openModal() {
  form.value = {
    customer_id: customers.value[0]?.id ?? '',
    device_id: '',
    title: '',
    description: '',
    priority: 'normal',
    channel: 'Telefon',
    assignee: store.user?.display_name ?? '',
    due_at: null,
  }
  formError.value = ''
  modalOpen.value = true
}

async function save() {
  if (!form.value.customer_id || !form.value.title.trim()) {
    formError.value = 'Bitte Kunde und Titel angeben.'
    return
  }

  saving.value = true
  formError.value = ''
  try {
    const result = await api.createTicket(form.value)
    modalOpen.value = false
    store.toast(`Einsatz angelegt: ${result.title}`, 'success')
    await load()
    router.push(`/wartungen/${result.ticket_id}`)
  } catch (caught) {
    formError.value = errorMessage(caught)
  } finally {
    saving.value = false
  }
}

function openTicket(id: string) {
  router.push(`/wartungen/${id}`)
}

onMounted(load)
</script>

<template>
  <div class="page-stack">
    <div class="page-toolbar page-toolbar--wrap">
      <div class="filter-chips">
        <button class="filter-chip" :class="{ 'is-active': filter === 'open' }" @click="filter = 'open'">
          Offen
          <span>{{ counts.open }}</span>
        </button>
        <button class="filter-chip" :class="{ 'is-active': filter === 'all' }" @click="filter = 'all'">
          Alle
          <span>{{ tickets.length }}</span>
        </button>
        <button class="filter-chip" :class="{ 'is-active': filter === 'done' }" @click="filter = 'done'">
          Erledigt
          <span>{{ counts.done }}</span>
        </button>
      </div>
      <div class="toolbar-actions">
        <a class="btn btn-ghost" :href="TIX_URL" target="_blank" rel="noreferrer" title="tix-Ticketsystem im neuen Tab öffnen">
          <ExternalLink :size="15" />
          In tix nachsehen
        </a>
        <button class="btn btn-ghost" :disabled="loading" @click="load">
          <RefreshCw :size="15" :class="{ spin: loading }" />
          Aktualisieren
        </button>
        <button class="btn btn-primary" @click="openModal">
          <Plus :size="16" />
          Einsatz anlegen
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

    <template v-else>
      <div class="kpi-grid kpi-grid--four">
        <article class="kpi-card">
          <div class="kpi-icon kpi-icon--amber"><Inbox :size="20" /></div>
          <div>
            <p class="kpi-label">Offene Einsätze</p>
            <strong class="kpi-value">{{ counts.open }}</strong>
            <p class="kpi-hint">in Bearbeitung oder neu</p>
          </div>
        </article>
        <article class="kpi-card">
          <div class="kpi-icon kpi-icon--teal"><CheckCircle2 :size="20" /></div>
          <div>
            <p class="kpi-label">Erledigt</p>
            <strong class="kpi-value">{{ counts.done }}</strong>
            <p class="kpi-hint">mit Zeit und Protokoll</p>
          </div>
        </article>
        <article class="kpi-card">
          <div class="kpi-icon kpi-icon--blue"><ShieldAlert :size="20" /></div>
          <div>
            <p class="kpi-label">Hohe Priorität</p>
            <strong class="kpi-value">{{ counts.urgent }}</strong>
            <p class="kpi-hint">offen und dringend</p>
          </div>
        </article>
        <article class="kpi-card">
          <div class="kpi-icon kpi-icon--violet"><Timer :size="20" /></div>
          <div>
            <p class="kpi-label">Zeiterfassung</p>
            <strong class="kpi-value">aktiv</strong>
            <p class="kpi-hint">Timer läuft pro Einsatz</p>
          </div>
        </article>
      </div>

      <div v-if="visible.length" class="card table-card">
        <table class="data-table">
          <thead>
            <tr>
              <th>Priorität</th>
              <th>Kunde</th>
              <th>Einsatz</th>
              <th>Kanal</th>
              <th class="num">Alter</th>
              <th>Status</th>
              <th aria-label="Öffnen" />
            </tr>
          </thead>
          <tbody>
            <tr v-for="ticket in visible" :key="ticket.id" tabindex="0" @click="openTicket(ticket.id)" @keydown.enter="openTicket(ticket.id)">
              <td><span class="badge" :class="`badge--priority-${ticket.priority}`">{{ priorityLabel(ticket.priority) }}</span></td>
              <td>
                <div class="stacked-cell">
                  <span>{{ ticket.customer_short_code }}</span>
                  <small>{{ ticket.customer_name }}</small>
                </div>
              </td>
              <td>
                <div class="stacked-cell">
                  <span>{{ ticket.title }}</span>
                  <small>{{ ticket.description ?? '–' }}</small>
                </div>
              </td>
              <td>{{ ticket.channel ?? '–' }}</td>
              <td class="num">{{ ageDays(ticket.scheduled_for) }} T</td>
              <td>
                <span
                  class="status-pill"
                  :class="ticket.status === 'done' ? 'status-pill--ok' : ticket.status === 'in_progress' ? 'status-pill--warning' : 'status-pill--muted'"
                >
                  {{ ticket.status === 'done' ? 'Erledigt' : ticket.status === 'in_progress' ? 'In Arbeit' : 'Neu' }}
                </span>
              </td>
              <td class="chevron-cell"><ArrowUpRight :size="15" /></td>
            </tr>
          </tbody>
        </table>
      </div>

      <div v-else class="card empty-state">
        <Inbox :size="28" />
        <h2>Keine Einsätze in dieser Ansicht</h2>
        <p>
          Einsätze sind spontane Arbeiten mit Zeit und Protokoll: Störung, Anfrage, Vor-Ort-Termin.
          Lege den ersten über „Einsatz anlegen" an.
        </p>
      </div>

      <section class="info-strip">
        <div class="info-strip-icon"><CalendarClock :size="18" /></div>
        <p>
          <strong>Einsätze sind keine Wartungen.</strong>
          Sie laufen über dasselbe Protokoll und dieselbe Zeiterfassung, zählen aber separat in der
          Übersicht. Ein vollständiges Helpdesk mit Mail-Eingang, SLA und Kundenportal bleibt bewusst
          draußen – das leistet NinjaOne bereits.
        </p>
      </section>
    </template>

    <Modal
      :open="modalOpen"
      title="Einsatz anlegen"
      subtitle="Spontane Arbeit erfassen – mit Kunde, Priorität und Zeit."
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
          <span>Titel</span>
          <input v-model="form.title" type="text" placeholder="z. B. Drucker im Empfang druckt nicht" required />
        </label>

        <label class="field">
          <span>Priorität</span>
          <select v-model="form.priority">
            <option v-for="priority in priorities" :key="priority" :value="priority">
              {{ priorityLabel(priority) }}
            </option>
          </select>
        </label>

        <label class="field">
          <span>Eingang über</span>
          <select v-model="form.channel">
            <option v-for="channel in channels" :key="channel" :value="channel">{{ channel }}</option>
          </select>
        </label>

        <label class="field">
          <span>Bearbeiter</span>
          <input v-model="form.assignee" type="text" />
        </label>

        <label class="field">
          <span>Fällig am (optional)</span>
          <input v-model="form.due_at" type="date" />
        </label>

        <label class="field field--full">
          <span>Beschreibung</span>
          <textarea v-model="form.description" rows="3" placeholder="Was ist passiert, was wurde vereinbart?" />
        </label>

        <p v-if="formError" class="form-error field--full">{{ formError }}</p>
      </form>

      <template #footer>
        <button class="btn btn-ghost" type="button" @click="modalOpen = false">Abbrechen</button>
        <button class="btn btn-primary" type="button" :disabled="saving" @click="save">
          {{ saving ? 'Speichert …' : 'Einsatz anlegen' }}
        </button>
      </template>
    </Modal>
  </div>
</template>
