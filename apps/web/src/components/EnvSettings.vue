<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { PlugZap, RefreshCw, Save, ServerCog } from 'lucide-vue-next'
import { api, errorMessage, isDesktop, restartApp } from '../api'
import { useAppStore } from '../stores/app'

const store = useAppStore()

const loading = ref(true)
const saving = ref(false)
const testing = ref(false)
const error = ref('')
const result = ref<{ ok: boolean; text: string } | null>(null)
const settingsFile = ref('')
const secretSet = ref(false)
const userSecretSet = ref(false)
const savedNeedsRestart = ref(false)

const form = ref({
  mock: true,
  base_url: 'https://eu.ninjarmm.com',
  client_id: '',
  client_secret: '',
  scope: 'monitoring management',
  user_client_id: '',
  user_client_secret: '',
  redirect_uri: '',
})

const canSave = computed(() => !!store.isAdmin)

const secretPlaceholder = computed(() =>
  secretSet.value ? 'unverändert lassen' : 'Client-Secret eingeben',
)
const userSecretPlaceholder = computed(() =>
  userSecretSet.value ? 'unverändert lassen' : 'optional',
)

async function load() {
  loading.value = true
  error.value = ''
  try {
    const data = await api.envSettings()
    form.value = {
      mock: data.mock,
      base_url: data.base_url || 'https://eu.ninjarmm.com',
      client_id: data.client_id,
      client_secret: '',
      scope: data.scope || 'monitoring management',
      user_client_id: data.user_client_id,
      user_client_secret: '',
      redirect_uri: data.redirect_uri,
    }
    secretSet.value = data.client_secret_set
    userSecretSet.value = data.user_client_secret_set
    settingsFile.value = data.file
  } catch (caught) {
    error.value = errorMessage(caught)
  } finally {
    loading.value = false
  }
}

/** Nur die Felder, die die API erwartet. */
function payload() {
  return {
    mock: form.value.mock,
    base_url: form.value.base_url,
    client_id: form.value.client_id,
    client_secret: form.value.client_secret || undefined,
    scope: form.value.scope,
    user_client_id: form.value.user_client_id,
    user_client_secret: form.value.user_client_secret || undefined,
    redirect_uri: form.value.redirect_uri,
  }
}

async function testConnection() {
  testing.value = true
  result.value = null
  try {
    const data = await api.testEnvSettings(payload())
    result.value = { ok: data.ok, text: data.report ?? (data.ok ? 'Verbindung steht.' : 'Fehlgeschlagen.') }
  } catch (caught) {
    result.value = { ok: false, text: errorMessage(caught) }
  } finally {
    testing.value = false
  }
}

async function save() {
  saving.value = true
  result.value = null
  try {
    const data = await api.updateEnvSettings(payload())
    result.value = { ok: true, text: data.report ?? 'Gespeichert.' }
    savedNeedsRestart.value = true
    if (form.value.client_secret) secretSet.value = true
    if (form.value.user_client_secret) userSecretSet.value = true
    form.value.client_secret = ''
    form.value.user_client_secret = ''
    store.toast('NinjaOne-Zugang gespeichert.', 'success')
  } catch (caught) {
    result.value = { ok: false, text: errorMessage(caught) }
  } finally {
    saving.value = false
  }
}

onMounted(load)
</script>

<template>
  <div v-if="loading" class="skeleton skeleton--panel" />

  <div v-else-if="error && !settingsFile" class="card empty-state">
    <p>{{ error }}</p>
    <button class="btn btn-primary" @click="load">Erneut versuchen</button>
  </div>

  <template v-else>
    <section class="card">
      <header class="card-head">
        <div>
          <h2>NinjaOne-Zugang</h2>
          <p>Client-ID, Secret und Adresse – hier änderbar, ohne Datei zu suchen</p>
        </div>
        <span class="status-pill" :class="form.mock ? 'status-pill--muted' : 'status-pill--ok'">
          <span class="badge-dot" />
          {{ form.mock ? 'Mock-Modus' : 'Live' }}
        </span>
      </header>

      <label class="check-row env-mode">
        <input v-model="form.mock" type="checkbox" />
        <span>
          <strong>Mock-Modus</strong> – keine Verbindung zu NinjaOne, Demo-Daten für Schulung
          und Test. Für den Echtbetrieb ausschalten.
        </span>
      </label>

      <div class="db-grid">
        <label class="field field--wide">
          <span>Server-Adresse</span>
          <input v-model="form.base_url" type="text" placeholder="https://eu.ninjarmm.com" />
          <small class="field-hint">
            EU-Mandanten: <code>https://eu.ninjarmm.com</code>, US-Mandanten:
            <code>https://app.ninjarmm.com</code>.
          </small>
        </label>

        <label class="field">
          <span>Client-ID</span>
          <input v-model="form.client_id" type="text" autocomplete="off" />
        </label>

        <label class="field">
          <span>Client-Secret</span>
          <input
            v-model="form.client_secret"
            type="password"
            autocomplete="new-password"
            :placeholder="secretPlaceholder"
          />
        </label>

        <label class="field">
          <span>Berechtigungen (Scope)</span>
          <input v-model="form.scope" type="text" placeholder="monitoring management" />
          <small class="field-hint">
            <code>monitoring management</code> reicht für Lesen, Patches, Neustart und
            Wartungsfenster.
          </small>
        </label>
      </div>

      <h3 class="env-subhead">Benutzer-App (nur für Scripts)</h3>
      <p class="settings-note">
        NinjaOne führt Scripts nur für einen echten Benutzer aus. Dafür braucht es eine zweite
        App vom Typ <em>Webanwendung</em> mit Rückleitung und Berechtigung für
        <code>Authorization Code</code> und <code>Refresh Token</code>. Ohne diese Angaben
        funktioniert alles außer dem Script-Test.
      </p>

      <div class="db-grid">
        <label class="field">
          <span>Client-ID</span>
          <input v-model="form.user_client_id" type="text" autocomplete="off" />
        </label>

        <label class="field">
          <span>Client-Secret</span>
          <input
            v-model="form.user_client_secret"
            type="password"
            autocomplete="new-password"
            :placeholder="userSecretPlaceholder"
          />
        </label>

        <label class="field field--wide">
          <span>Rückleitungs-Adresse</span>
          <input v-model="form.redirect_uri" type="text" />
          <small class="field-hint">
            Muss in der NinjaOne-App genauso hinterlegt sein. Beim Browserbetrieb auf Port 8080,
            in der Desktop-App auf die dort eingetragene Adresse.
          </small>
        </label>
      </div>

      <p v-if="result" class="db-result" :class="result.ok ? 'db-result--ok' : 'db-result--fail'">
        <PlugZap :size="15" />
        <span>{{ result.text }}</span>
      </p>

      <p v-if="savedNeedsRestart" class="db-restart">
        <ServerCog :size="15" />
        <span>
          Gespeichert in <code>{{ settingsFile }}</code>. Die Zugänge gelten nach einem Neustart
          der Anwendung – bis dahin läuft alles mit den bisherigen.
        </span>
        <button v-if="isDesktop" class="btn btn-sm btn-primary" @click="restartApp">
          Jetzt neu starten
        </button>
      </p>

      <div class="settings-foot">
        <p class="settings-note">
          Gespeichert wird in <code>{{ settingsFile }}</code>, nicht in der Datenbank. Die Datei
          bleibt auf jedem Rechner liegen; im Installer stecken keine Zugangsdaten.
        </p>
        <div class="db-actions">
          <button class="btn btn-ghost" :disabled="testing || !canSave" @click="testConnection">
            <PlugZap :size="15" />
            {{ testing ? 'Prüft …' : 'Verbindung testen' }}
          </button>
          <button class="btn btn-primary" :disabled="saving || !canSave" @click="save">
            <Save :size="15" />
            {{ saving ? 'Speichert …' : 'Speichern' }}
          </button>
          <button class="icon-btn" title="Neu laden" :disabled="loading" @click="load">
            <RefreshCw :size="15" />
          </button>
        </div>
      </div>
    </section>

    <p v-if="!canSave" class="hint-line">
      Nur Administratoren dürfen die NinjaOne-Zugänge sehen und ändern.
    </p>
  </template>
</template>
