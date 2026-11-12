<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import {
  Building2,
  Calculator,
  Database,
  MonitorSmartphone,
  RefreshCw,
  Save,
  Server,
  UserCog,
} from 'lucide-vue-next'
import { api, errorMessage } from '../api'
import { currency } from '../format'
import type { BillingSettings } from '../types'
import UsersView from './UsersView.vue'
import AccountSettings from '../components/AccountSettings.vue'
import DatabaseSettings from '../components/DatabaseSettings.vue'
import EnvSettings from '../components/EnvSettings.vue'
import TeamViewerSettings from '../components/TeamViewerSettings.vue'
import { useAppStore } from '../stores/app'

const route = useRoute()
const router = useRouter()
const store = useAppStore()

type Tab = 'benutzer' | 'abrechnung' | 'konten' | 'datenbank' | 'ninjaone' | 'teamviewer'

const tabs: Array<{ key: Tab; label: string; icon: typeof UserCog }> = [
  { key: 'benutzer', label: 'Benutzer', icon: UserCog },
  { key: 'abrechnung', label: 'Stundensätze', icon: Calculator },
  { key: 'konten', label: 'Konten', icon: Building2 },
  { key: 'datenbank', label: 'Datenbank', icon: Database },
  { key: 'ninjaone', label: 'NinjaOne', icon: Server },
  { key: 'teamviewer', label: 'TeamViewer', icon: MonitorSmartphone },
]

const activeTab = computed<Tab>(() => {
  const tab = route.query.tab
  if (
    tab === 'abrechnung' ||
    tab === 'konten' ||
    tab === 'datenbank' ||
    tab === 'ninjaone' ||
    tab === 'teamviewer'
  ) {
    return tab
  }
  return 'benutzer'
})

function selectTab(tab: Tab) {
  router.replace({ path: '/einstellungen', query: tab === 'benutzer' ? {} : { tab } })
}

// --- Abrechnungseinstellungen

const billing = ref<BillingSettings | null>(null)
const billingLoading = ref(true)
const saving = ref(false)
const error = ref('')

const draft = ref({ default_hourly_rate: 126.05, vat_rate: 19 })

/** Bruttobetrag, damit man den Satz sofort einordnen kann. */
const grossRate = computed(() => {
  const net = draft.value.default_hourly_rate || 0
  return net * (1 + (draft.value.vat_rate || 0) / 100)
})

const canSave = computed(() => !!store.isAdmin)

async function loadBilling() {
  billingLoading.value = true
  error.value = ''
  try {
    const settings = await api.billingSettings()
    billing.value = settings
    draft.value = {
      default_hourly_rate: settings.default_hourly_rate,
      vat_rate: settings.vat_rate,
    }
  } catch (caught) {
    error.value = errorMessage(caught)
  } finally {
    billingLoading.value = false
  }
}

async function save() {
  saving.value = true
  error.value = ''
  try {
    const updated = await api.updateBillingSettings(draft.value)
    billing.value = updated
    store.toast('Stundensätze gespeichert.', 'success')
  } catch (caught) {
    error.value = errorMessage(caught)
  } finally {
    saving.value = false
  }
}

onMounted(loadBilling)
</script>

<template>
  <div class="page-stack">
    <section class="hero hero--compact">
      <div>
        <p class="eyebrow">Einstellungen</p>
        <h2>Einstellungen</h2>
        <p class="hero-sub">
          Benutzer, Rollen und die Zahlen, mit denen gerechnet wird. Was hier steht, gilt für
          Übersicht, Bericht und Abrechnung gleichermaßen.
        </p>
      </div>
      <div class="hero-actions">
        <button v-if="activeTab === 'abrechnung'" class="btn btn-ghost" :disabled="billingLoading" @click="loadBilling">
          <RefreshCw :size="15" :class="{ spin: billingLoading }" />
          Aktualisieren
        </button>
      </div>
    </section>

    <nav class="settings-tabs">
      <button
        v-for="tab in tabs"
        :key="tab.key"
        class="settings-tab"
        :class="{ 'is-active': activeTab === tab.key }"
        @click="selectTab(tab.key)"
      >
        <component :is="tab.icon" :size="16" />
        {{ tab.label }}
      </button>
    </nav>

    <UsersView v-if="activeTab === 'benutzer'" />
    <AccountSettings v-else-if="activeTab === 'konten'" />

    <DatabaseSettings v-else-if="activeTab === 'datenbank'" />

    <EnvSettings v-else-if="activeTab === 'ninjaone'" />
    <TeamViewerSettings v-else-if="activeTab === 'teamviewer'" />

    <template v-else-if="activeTab === 'abrechnung'">
      <div v-if="billingLoading" class="skeleton skeleton--panel" />

      <div v-else-if="error && !billing" class="card empty-state">
        <p>{{ error }}</p>
        <button class="btn btn-primary" @click="loadBilling">Erneut versuchen</button>
      </div>

      <section v-else class="card">
        <header class="card-head">
          <div>
            <h2>Stundensätze</h2>
            <p>Standardwerte, wenn im Vertrag eines Kunden nichts gepflegt ist</p>
          </div>
        </header>

        <div class="settings-grid">
          <label class="field">
            <span>Stundensatz netto</span>
            <div class="dauer-input">
              <input
                v-model.number="draft.default_hourly_rate"
                type="number"
                min="1"
                max="1000"
                step="0.01"
              />
              <span>€</span>
            </div>
            <small class="field-hint">
              Entspricht {{ currency(grossRate) }} brutto bei {{ draft.vat_rate }} % USt.
            </small>
          </label>

          <label class="field">
            <span>Umsatzsteuer</span>
            <div class="dauer-input">
              <input v-model.number="draft.vat_rate" type="number" min="0" max="30" step="0.1" />
              <span>%</span>
            </div>
            <small class="field-hint">Steht als eigene Zeile im Monatsbericht.</small>
          </label>

        </div>

        <p v-if="error" class="form-error">{{ error }}</p>

        <div class="settings-foot">
          <p class="settings-note">
            Kunden mit eigenem Satz im Vertrag behalten ihren Satz. Diese Werte gelten nur, wo
            keiner hinterlegt ist – im Bericht steht dann „Standard".
          </p>
          <button class="btn btn-primary" :disabled="saving || !canSave" @click="save">
            <Save :size="15" />
            {{ saving ? 'Speichert …' : 'Speichern' }}
          </button>
        </div>
      </section>

      <p v-if="!canSave" class="hint-line">
        Nur Administratoren dürfen Stundensätze ändern.
      </p>
    </template>
  </div>
</template>
