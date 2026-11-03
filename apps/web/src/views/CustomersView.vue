<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useRouter } from 'vue-router'
import { ChevronRight, Eye, EyeOff, Search, ShieldAlert, Users } from 'lucide-vue-next'
import { api, errorMessage } from '../api'
import type { CustomerSummary } from '../types'
import { formatHours, initials } from '../format'
import { useAppStore } from '../stores/app'

const router = useRouter()
const store = useAppStore()
const customers = ref<CustomerSummary[]>([])
const search = ref('')
const loading = ref(true)
const error = ref('')
const view = ref<'focus' | 'hidden' | 'all'>('focus')
const busy = ref<string | null>(null)

const counts = computed(() => ({
  focus: customers.value.filter((customer) => customer.focus).length,
  hidden: customers.value.filter((customer) => !customer.focus).length,
  all: customers.value.length,
}))

const filtered = computed(() => {
  const query = search.value.trim().toLowerCase()
  return customers.value
    .filter((customer) => {
      if (view.value === 'focus') return customer.focus
      if (view.value === 'hidden') return !customer.focus
      return true
    })
    .filter((customer) => {
      if (!query) return true
      return [customer.name, customer.short_code, customer.contact_name, customer.contact_email]
        .filter(Boolean)
        .some((value) => value!.toLowerCase().includes(query))
    })
    .sort((a, b) => b.device_count - a.device_count)
})

async function load() {
  loading.value = true
  error.value = ''
  try {
    customers.value = await api.customers()
  } catch (caught) {
    error.value = errorMessage(caught)
  } finally {
    loading.value = false
  }
}

async function toggleFocus(customer: CustomerSummary, event: Event) {
  event.stopPropagation()
  busy.value = customer.id
  try {
    const updated = await api.setCustomerFocus(customer.id, !customer.focus)
    customers.value = customers.value.map((entry) => (entry.id === updated.id ? updated : entry))
    store.toast(
      updated.focus
        ? `${updated.name} ist wieder im Fokus.`
        : `${updated.name} wird in der Übersicht ausgeblendet.`,
      'success',
    )
  } catch (caught) {
    store.toast(errorMessage(caught), 'error')
  } finally {
    busy.value = null
  }
}

function openCustomer(id: string) {
  router.push(`/kunden/${id}`)
}

onMounted(load)
</script>

<template>
  <div class="page-stack page-fill">
    <div class="page-toolbar page-toolbar--wrap">
      <div class="filter-chips">
        <button class="filter-chip" :class="{ 'is-active': view === 'focus' }" @click="view = 'focus'">
          Fokus
          <span>{{ counts.focus }}</span>
        </button>
        <button class="filter-chip" :class="{ 'is-active': view === 'hidden' }" @click="view = 'hidden'">
          Ausgeblendet
          <span>{{ counts.hidden }}</span>
        </button>
        <button class="filter-chip" :class="{ 'is-active': view === 'all' }" @click="view = 'all'">
          Alle
          <span>{{ counts.all }}</span>
        </button>
      </div>

      <div class="toolbar-actions">
        <div class="search-box">
          <Search :size="17" />
          <input v-model="search" type="search" placeholder="Kunde, Ansprechpartner, Kürzel …" />
        </div>
      </div>
    </div>

    <p class="hint-line">
      <Users :size="15" />
      Ausgeblendete Kunden bleiben vollständig verwaltet – sie fehlen nur in der Übersicht und in
      den Kennzahlen. Geräte, Zeiten und Berichte bleiben unverändert.
    </p>

    <div v-if="loading" class="skeleton skeleton--panel" />
    <div v-else-if="error" class="card empty-state">
      <ShieldAlert :size="26" />
      <p>{{ error }}</p>
      <button class="btn btn-primary" @click="load">Erneut versuchen</button>
    </div>

    <div v-else class="card table-card scroll-area">
      <table class="data-table">
        <thead>
          <tr>
            <th>Kunde</th>
            <th>Ansprechpartner</th>
            <th class="num">Geräte</th>
            <th class="num">Sites</th>
            <th class="num">Offene Vorgänge</th>
            <th class="num">Stunden / Monat</th>
            <th>Status</th>
            <th class="center">Übersicht</th>
            <th aria-label="Öffnen" />
          </tr>
        </thead>
        <tbody>
          <tr
            v-for="customer in filtered"
            :key="customer.id"
            tabindex="0"
            :class="{ 'row--muted': !customer.focus }"
            @click="openCustomer(customer.id)"
            @keydown.enter="openCustomer(customer.id)"
          >
            <td>
              <div class="entity-cell">
                <div class="avatar">{{ initials(customer.name) }}</div>
                <div>
                  <strong>{{ customer.name }}</strong>
                  <small>{{ customer.short_code }} · NinjaOne #{{ customer.ninja_org_id ?? '–' }}</small>
                </div>
              </div>
            </td>
            <td>
              <div class="stacked-cell">
                <span>{{ customer.contact_name ?? '–' }}</span>
                <small>{{ customer.contact_email ?? customer.phone ?? '' }}</small>
              </div>
            </td>
            <td class="num">{{ customer.device_count }}</td>
            <td class="num">{{ customer.site_count }}</td>
            <td class="num">
              <span :class="{ 'num-strong': customer.open_runs > 0 }">{{ customer.open_runs }}</span>
            </td>
            <td class="num">{{ formatHours(customer.month_hours) }}</td>
            <td>
              <span
                class="status-pill"
                :class="customer.issues > 0 ? 'status-pill--warning' : 'status-pill--ok'"
              >
                <span class="badge-dot" />
                {{ customer.issues > 0 ? 'Handlungsbedarf' : 'Stabil' }}
              </span>
            </td>
            <td class="center">
              <button
                class="icon-btn"
                :class="customer.focus ? 'is-focus' : 'is-muted'"
                :disabled="busy === customer.id"
                :aria-label="customer.focus ? `${customer.name} ausblenden` : `${customer.name} einblenden`"
                :title="customer.focus ? 'Aus Übersicht ausblenden' : 'In Übersicht einblenden'"
                @click="toggleFocus(customer, $event)"
              >
                <Eye v-if="customer.focus" :size="16" />
                <EyeOff v-else :size="16" />
              </button>
            </td>
            <td class="chevron-cell"><ChevronRight :size="17" /></td>
          </tr>
        </tbody>
      </table>

      <div v-if="!filtered.length" class="empty-state empty-state--compact">
        <p>Kein Kunde in dieser Ansicht.</p>
      </div>
    </div>
  </div>
</template>
