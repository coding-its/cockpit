<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useRouter } from 'vue-router'
import {
  ArrowUpRight,
  Clock3,
  Globe2,
  Plus,
  RefreshCw,
  Search,
  ShieldAlert,
} from 'lucide-vue-next'
import { api, errorMessage } from '../api'
import type { CreateWebClientPayload, WebClientSummary } from '../types'
import { formatHours, initials, relativeTime } from '../format'
import Modal from '../components/Modal.vue'
import { useAppStore } from '../stores/app'

const router = useRouter()
const store = useAppStore()
const clients = ref<WebClientSummary[]>([])
const loading = ref(true)
const error = ref('')
const search = ref('')
const modalOpen = ref(false)
const saving = ref(false)
const formError = ref('')

const form = ref<CreateWebClientPayload>({
  name: '',
  short_code: '',
  contact_name: '',
  contact_email: '',
  phone: '',
  website_url: '',
  notes: '',
  hourly_rate: 95,
})

const filtered = computed(() => {
  const query = search.value.trim().toLowerCase()
  if (!query) return clients.value
  return clients.value.filter((client) =>
    [client.name, client.short_code, client.contact_name, client.contact_email, client.website_url]
      .filter(Boolean)
      .some((value) => value!.toLowerCase().includes(query)),
  )
})

const totals = computed(() => ({
  sites: clients.value.reduce((sum, client) => sum + client.site_count, 0),
  hours: clients.value.reduce((sum, client) => sum + client.month_hours, 0),
  updates: clients.value.reduce((sum, client) => sum + client.plugin_updates, 0),
}))

async function load() {
  loading.value = true
  error.value = ''
  try {
    clients.value = await api.webClients()
  } catch (caught) {
    error.value = errorMessage(caught)
  } finally {
    loading.value = false
  }
}

function openModal() {
  form.value = {
    name: '',
    short_code: '',
    contact_name: '',
    contact_email: '',
    phone: '',
    website_url: '',
    notes: '',
    hourly_rate: 95,
  }
  formError.value = ''
  modalOpen.value = true
}

async function save() {
  if (!form.value.name.trim()) {
    formError.value = 'Bitte einen Namen angeben.'
    return
  }

  saving.value = true
  formError.value = ''
  try {
    const created = await api.createWebClient(form.value)
    modalOpen.value = false
    store.toast(`Web-Kunde angelegt: ${created.name}`, 'success')
    await load()
    router.push(`/websites/${created.id}`)
  } catch (caught) {
    formError.value = errorMessage(caught)
  } finally {
    saving.value = false
  }
}

onMounted(load)
</script>

<template>
  <div class="page-stack page-fill">
    <section class="hero hero--compact">
      <div>
        <p class="eyebrow">WordPress-Betreuung</p>
        <h2>Websites</h2>
        <p class="hero-sub">
          Eigener Kundenstamm für WordPress: Zugangsdaten, Aktionen und Zeiten – getrennt von den
          NinjaOne-Kunden.
        </p>
      </div>
      <div class="hero-actions">
        <button class="btn btn-ghost" :disabled="loading" @click="load">
          <RefreshCw :size="15" :class="{ spin: loading }" />
          Aktualisieren
        </button>
        <button class="btn btn-primary" @click="openModal">
          <Plus :size="16" />
          Web-Kunde anlegen
        </button>
      </div>
    </section>

    <section class="kpi-grid kpi-grid--four">
      <article class="kpi-card">
        <div class="kpi-icon kpi-icon--blue"><Globe2 :size="20" /></div>
        <div>
          <p class="kpi-label">Web-Kunden</p>
          <strong class="kpi-value">{{ clients.length }}</strong>
          <p class="kpi-hint">{{ totals.sites }} Websites</p>
        </div>
      </article>
      <article class="kpi-card">
        <div class="kpi-icon kpi-icon--teal"><Clock3 :size="20" /></div>
        <div>
          <p class="kpi-label">Stunden im Monat</p>
          <strong class="kpi-value">{{ formatHours(totals.hours) }}</strong>
          <p class="kpi-hint">abrechenbar</p>
        </div>
      </article>
      <article class="kpi-card">
        <div class="kpi-icon kpi-icon--amber"><ArrowUpRight :size="20" /></div>
        <div>
          <p class="kpi-label">Offene Plugin-Updates</p>
          <strong class="kpi-value">{{ totals.updates }}</strong>
          <p class="kpi-hint">über alle Websites</p>
        </div>
      </article>
      <article class="kpi-card">
        <div class="kpi-icon kpi-icon--violet"><ShieldAlert :size="20" /></div>
        <div>
          <p class="kpi-label">Auffällige Seiten</p>
          <strong class="kpi-value">{{ clients.reduce((sum, c) => sum + c.issues, 0) }}</strong>
          <p class="kpi-hint">Warnung oder kritisch</p>
        </div>
      </article>
    </section>

    <div class="page-toolbar page-toolbar--wrap">
      <div class="search-box">
        <Search :size="17" />
        <input v-model="search" type="search" placeholder="Name, Kontakt, Adresse …" />
      </div>
      <div class="toolbar-meta">
        {{ filtered.length }} von {{ clients.length }} Web-Kunden
      </div>
    </div>

    <div v-if="loading" class="skeleton skeleton--panel" />
    <div v-else-if="error" class="card empty-state">
      <ShieldAlert :size="26" />
      <p>{{ error }}</p>
      <button class="btn btn-primary" @click="load">Erneut versuchen</button>
    </div>

    <template v-else>
      <div v-if="filtered.length" class="card table-card scroll-area">
        <table class="data-table">
          <thead>
            <tr>
              <th>Web-Kunde</th>
              <th>Kontakt</th>
              <th class="num">Websites</th>
              <th class="num">Updates</th>
              <th class="num">Stunden / Monat</th>
              <th class="num">Gesamt</th>
              <th>Letzte Arbeit</th>
              <th aria-label="Öffnen" />
            </tr>
          </thead>
          <tbody>
            <tr
              v-for="client in filtered"
              :key="client.id"
              tabindex="0"
              @click="router.push(`/websites/${client.id}`)"
              @keydown.enter="router.push(`/websites/${client.id}`)"
            >
              <td>
                <div class="entity-cell">
                  <div class="avatar">{{ initials(client.name) }}</div>
                  <div>
                    <strong>{{ client.name }}</strong>
                    <small>{{ client.short_code }}{{ client.website_url ? ` · ${client.website_url}` : '' }}</small>
                  </div>
                </div>
              </td>
              <td>
                <div class="stacked-cell">
                  <span>{{ client.contact_name ?? '–' }}</span>
                  <small>{{ client.contact_email ?? client.phone ?? '' }}</small>
                </div>
              </td>
              <td class="num">{{ client.site_count }}</td>
              <td class="num">
                <span :class="{ 'num-strong': client.plugin_updates > 0 }">{{ client.plugin_updates }}</span>
              </td>
              <td class="num">{{ formatHours(client.month_hours) }}</td>
              <td class="num">{{ formatHours(client.total_hours) }}</td>
              <td>
                <span v-if="client.last_activity">{{ relativeTime(client.last_activity) }}</span>
                <span v-else class="muted">–</span>
              </td>
              <td class="chevron-cell"><ArrowUpRight :size="16" /></td>
            </tr>
          </tbody>
        </table>
      </div>

      <div v-else class="card empty-state">
        <Globe2 :size="28" />
        <h2>Noch keine Web-Kunden</h2>
        <p>
          Lege den ersten WordPress-Kunden an. Die Adresse der Website und die Zugangsdaten kommen
          danach im Kundendetail dazu.
        </p>
        <button class="btn btn-primary" @click="openModal">
          <Plus :size="16" />
          Web-Kunde anlegen
        </button>
      </div>

      <section class="info-strip">
        <div class="info-strip-icon"><Globe2 :size="18" /></div>
        <p>
          <strong>Getrennter Kundenstamm.</strong>
          WordPress-Kunden liegen bewusst nicht bei den NinjaOne-Kunden: andere Stammdaten, eigene
          Zugangsdaten, eigene Zeiten. In der Abrechnung laufen beide zusammen.
        </p>
      </section>
    </template>

    <Modal
      :open="modalOpen"
      title="Web-Kunde anlegen"
      subtitle="Stammdaten für einen WordPress-Kunden. Die Website folgt im Detail."
      @close="modalOpen = false"
    >
      <form class="form-grid" @submit.prevent="save">
        <label class="field field--full">
          <span>Name</span>
          <input v-model="form.name" type="text" placeholder="z. B. Musterfirma GmbH" required />
        </label>

        <label class="field">
          <span>Kürzel (optional)</span>
          <input v-model="form.short_code" type="text" placeholder="automatisch" maxlength="6" />
        </label>

        <label class="field">
          <span>Stundensatz</span>
          <input v-model.number="form.hourly_rate" type="number" min="0" step="5" />
        </label>

        <label class="field field--full">
          <span>Ansprechpartner</span>
          <input v-model="form.contact_name" type="text" />
        </label>

        <label class="field">
          <span>E-Mail</span>
          <input v-model="form.contact_email" type="email" />
        </label>

        <label class="field">
          <span>Telefon</span>
          <input v-model="form.phone" type="tel" />
        </label>

        <label class="field field--full">
          <span>Website (optional)</span>
          <input v-model="form.website_url" type="text" placeholder="https://www.example.de" />
        </label>

        <label class="field field--full">
          <span>Notizen</span>
          <textarea v-model="form.notes" rows="2" placeholder="Hosting, Besonderheiten, Freigaben …" />
        </label>

        <p v-if="formError" class="form-error field--full">{{ formError }}</p>
      </form>

      <template #footer>
        <button class="btn btn-ghost" type="button" @click="modalOpen = false">Abbrechen</button>
        <button class="btn btn-primary" type="button" :disabled="saving" @click="save">
          {{ saving ? 'Speichert …' : 'Anlegen' }}
        </button>
      </template>
    </Modal>
  </div>
</template>
