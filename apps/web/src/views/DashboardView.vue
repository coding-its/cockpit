<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { RouterLink } from 'vue-router'
import {
  ArrowRight,
  ArrowUpRight,
  CalendarClock,
  Clock3,
  Euro,
  Globe2,
  RefreshCw,
  ShieldAlert,
  Users,
  Wrench,
} from 'lucide-vue-next'
import { api, errorMessage } from '../api'
import type { CustomerSummary, Dashboard } from '../types'
import { currency, formatHours, greeting, monthLabel, relativeTime } from '../format'
import RunCard from '../components/RunCard.vue'
import ProgressBar from '../components/ProgressBar.vue'
import { useAppStore } from '../stores/app'

const store = useAppStore()

const firstName = computed(() => {
  const name = store.user?.display_name?.trim()
  if (!name) return 'zusammen'
  // "Max Mustermann" -> "Max"; "Erika Muster" -> "Erika"
  return name.includes('.') ? name : name.split(/\s+/)[0]
})

const data = ref<Dashboard | null>(null)
const customers = ref<CustomerSummary[]>([])
const loading = ref(true)
const error = ref('')

const maxHours = computed(() =>
  Math.max(1, ...customers.value.map((customer) => customer.month_hours)),
)
const sortedCustomers = computed(() =>
  [...customers.value]
    .filter((customer) => customer.month_hours > 0)
    .sort((a, b) => b.month_hours - a.month_hours)
    .slice(0, 6),
)
const totalDevices = computed(() =>
  customers.value.reduce((sum, customer) => sum + customer.device_count, 0),
)

async function load() {
  loading.value = true
  error.value = ''
  try {
    const [dashboard, customerList] = await Promise.all([api.dashboard(), api.customers()])
    data.value = dashboard
    customers.value = customerList
  } catch (caught) {
    error.value = errorMessage(caught)
  } finally {
    loading.value = false
  }
}

onMounted(load)
</script>

<template>
  <div class="page-stack">
    <div v-if="loading" class="skeleton-stack">
      <div class="skeleton skeleton--hero" />
      <div class="kpi-grid">
        <div v-for="index in 4" :key="index" class="skeleton skeleton--kpi" />
      </div>
      <div class="skeleton skeleton--panel" />
    </div>

    <div v-else-if="error" class="card empty-state">
      <ShieldAlert :size="28" />
      <h2>Daten konnten nicht geladen werden</h2>
      <p>{{ error }}</p>
      <button class="btn btn-primary" @click="load">
        <RefreshCw :size="16" />
        Erneut versuchen
      </button>
    </div>

    <template v-else-if="data">
      <section class="hero">
        <div>
          <p class="eyebrow">Wartungssteuerung · {{ monthLabel(data.month) }}</p>
          <h2>{{ greeting() }}, {{ firstName }}.</h2>
          <p class="hero-sub">
            {{ data.open_runs }} Wartungen und {{ data.open_tickets }} Einsätze offen,
            {{ data.critical_sites }} WordPress-Sites brauchen Aufmerksamkeit<template
              v-if="data.pending_patches"
            >, {{ data.pending_patches }} Geräte haben offene Patches</template>.
            {{ data.customer_count }} Kunden im Fokus{{ data.hidden_customers ? `, ${data.hidden_customers} kleine Kunden ausgeblendet` : '' }}.
          </p>
        </div>
        <div class="hero-actions">
          <RouterLink to="/wartungen" class="btn btn-primary">
            Wartungen öffnen
            <ArrowRight :size="16" />
          </RouterLink>
          <RouterLink to="/zeiten" class="btn btn-ghost">Zeiten ansehen</RouterLink>
        </div>
      </section>

      <section class="kpi-grid">
        <article class="kpi-card">
          <div class="kpi-icon kpi-icon--amber"><Wrench :size="20" /></div>
          <div>
            <p class="kpi-label">Offene Vorgänge</p>
            <strong class="kpi-value">{{ data.open_runs + data.open_tickets }}</strong>
            <p class="kpi-hint">
              {{ data.open_runs }} Wartungen · {{ data.open_tickets }} Einsätze
            </p>
          </div>
        </article>
        <article class="kpi-card">
          <div class="kpi-icon kpi-icon--teal"><Clock3 :size="20" /></div>
          <div>
            <p class="kpi-label">Abrechenbare Stunden</p>
            <strong class="kpi-value">{{ formatHours(data.billable_hours) }}</strong>
            <p class="kpi-hint">im laufenden Monat</p>
          </div>
        </article>
        <article class="kpi-card">
          <div class="kpi-icon kpi-icon--violet"><Euro :size="20" /></div>
          <div>
            <p class="kpi-label">Abrechnungssumme</p>
            <strong class="kpi-value">{{ currency(data.billable_amount) }}</strong>
            <p class="kpi-hint">inkl. Pauschalbeträgen</p>
          </div>
        </article>
        <article class="kpi-card">
          <div class="kpi-icon kpi-icon--blue"><Globe2 :size="20" /></div>
          <div>
            <p class="kpi-label">WordPress-Sites</p>
            <strong class="kpi-value">{{ data.site_count }}</strong>
            <p class="kpi-hint">{{ data.critical_sites }} mit Handlungsbedarf</p>
          </div>
        </article>
      </section>

      <div class="dashboard-grid">
        <div class="stack">
          <section class="card">
            <header class="card-head">
              <div>
                <h2>Anstehende Wartungen</h2>
                <p>Nächste Läufe über alle Kunden</p>
              </div>
              <RouterLink to="/wartungen" class="link">
                Alle ansehen
                <ArrowUpRight :size="14" />
              </RouterLink>
            </header>

            <div v-if="data.next_runs.length" class="run-grid">
              <RunCard v-for="run in data.next_runs.slice(0, 3)" :key="run.id" :run="run" />
            </div>
            <div v-else class="empty-state empty-state--compact">
              <CalendarClock :size="24" />
              <p>Keine offenen Wartungen. Alles erledigt.</p>
            </div>
          </section>

          <section class="card">
            <header class="card-head">
              <div>
                <h2>Zeiten nach Kunde</h2>
                <p>Abrechenbare Stunden · {{ monthLabel(data.month) }}</p>
              </div>
              <RouterLink to="/zeiten" class="link">
                Details
                <ArrowUpRight :size="14" />
              </RouterLink>
            </header>

            <div v-if="sortedCustomers.length" class="bar-chart">
              <RouterLink
                v-for="customer in sortedCustomers"
                :key="customer.id"
                :to="`/kunden/${customer.id}`"
                class="bar-row"
              >
                <span class="bar-label">{{ customer.name }}</span>
                <div class="bar-track">
                  <div
                    class="bar-fill"
                    :style="{ width: `${Math.max(3, (customer.month_hours / maxHours) * 100)}%` }"
                  />
                </div>
                <strong class="bar-value">{{ formatHours(customer.month_hours) }}</strong>
              </RouterLink>
            </div>
            <div v-else class="empty-state empty-state--compact">
              <Clock3 :size="22" />
              <p>Noch keine Zeiten erfasst. Starte einen Timer in einem Wartungslauf.</p>
            </div>
          </section>
        </div>

        <div class="stack">
          <section class="card">
            <header class="card-head">
              <div>
                <h2>Handlungsbedarf</h2>
                <p>Sites und Geräte mit offenen Themen</p>
              </div>
              <span class="count-pill">{{ data.attention.length }}</span>
            </header>

            <div v-if="data.attention.length" class="attention-list">
              <RouterLink
                v-for="item in data.attention"
                :key="`${item.kind}-${item.customer_id}-${item.label}`"
                :to="`/kunden/${item.customer_id}`"
                class="attention-item"
                :class="`attention-item--${item.severity}`"
              >
                <span class="attention-icon">
                  <ShieldAlert v-if="item.severity === 'critical'" :size="16" />
                  <Wrench v-else :size="16" />
                </span>
                <div>
                  <strong>{{ item.label }}</strong>
                  <small>{{ item.customer_name }}</small>
                  <p>{{ item.detail }}</p>
                </div>
                <ArrowUpRight :size="15" class="attention-arrow" />
              </RouterLink>
            </div>
            <div v-else class="empty-state empty-state--compact">
              <p>Keine offenen Risiken.</p>
            </div>
          </section>

          <section class="card">
            <header class="card-head">
              <div>
                <h2>Letzte Aktivität</h2>
                <p>Audit-Trail aus dem Cockpit</p>
              </div>
            </header>

            <ol class="activity-list activity-list--preview">
              <li v-for="event in data.recent_activity" :key="event.id" class="activity-item">
                <span class="activity-dot" />
                <div>
                  <strong>{{ event.action }}</strong>
                  <p>{{ event.detail }}</p>
                  <small>{{ event.actor }} · {{ relativeTime(event.created_at) }}</small>
                </div>
              </li>
            </ol>
          </section>
        </div>
      </div>

      <section class="info-strip">
        <div class="info-strip-icon"><Users :size="18" /></div>
        <p v-if="store.isLive">
          <strong>Live-Daten aus NinjaOne.</strong>
          {{ data.customer_count }} Kunden im Fokus, {{ data.hidden_customers }} ausgeblendet – steuerbar
          unter <RouterLink to="/kunden" class="inline-link">Kunden</RouterLink>. Ein Klick auf
          "NinjaOne" oben rechts gleicht den Bestand erneut ab.
        </p>
        <p v-else>
          <strong>Kunden und Geräte kommen aus NinjaOne.</strong>
          Im Demo-Modus werden {{ data.customer_count }} Organisationen und die zugehörigen Geräte
          simuliert. Ein Klick auf "NinjaOne" oben rechts testet den Sync.
        </p>
        <RouterLink to="/integrationen" class="link">Integrationen</RouterLink>
      </section>
    </template>
  </div>
</template>
