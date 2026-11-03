<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { Database, PlugZap, Save, ServerCog } from 'lucide-vue-next'
import { api, errorMessage, isDesktop, restartApp } from '../api'
import { useAppStore } from '../stores/app'

const store = useAppStore()

const loading = ref(true)
const saving = ref(false)
const testing = ref(false)
const error = ref('')
const result = ref<{ ok: boolean; text: string } | null>(null)
const settingsFile = ref('')
const passwordSet = ref(false)
const savedNeedsRestart = ref(false)

const form = ref({
  host: '127.0.0.1',
  port: 5432,
  database: 'wartungscockpit',
  user: 'cockpit',
  password: '',
})

/** Passwort nur mitschicken, wenn eines eingetippt wurde. */
const passwordPlaceholder = computed(() =>
  passwordSet.value ? 'unverändert lassen' : 'Passwort eingeben',
)

const canSave = computed(() => !!store.isAdmin)

async function load() {
  loading.value = true
  error.value = ''
  try {
    const data = await api.databaseSettings()
    form.value = {
      host: data.host,
      port: data.port,
      database: data.database,
      user: data.user,
      password: '',
    }
    passwordSet.value = data.password_set
    settingsFile.value = data.file
  } catch (caught) {
    error.value = errorMessage(caught)
  } finally {
    loading.value = false
  }
}

async function testConnection() {
  testing.value = true
  result.value = null
  error.value = ''
  try {
    const data = await api.updateDatabaseSettings({ ...form.value, test_only: true })
    result.value = { ok: true, text: data.report ?? 'Verbindung steht.' }
  } catch (caught) {
    result.value = { ok: false, text: errorMessage(caught) }
  } finally {
    testing.value = false
  }
}

async function save() {
  saving.value = true
  result.value = null
  error.value = ''
  try {
    const data = await api.updateDatabaseSettings(form.value)
    result.value = { ok: true, text: data.report ?? 'Verbindung steht.' }
    savedNeedsRestart.value = true
    if (form.value.password) passwordSet.value = true
    form.value.password = ''
    store.toast('Datenbankverbindung gespeichert.', 'success')
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
          <h2>Datenbankverbindung</h2>
          <p>Wo die Kunden, Geräte und Zeiten liegen</p>
        </div>
        <span class="status-pill status-pill--ok">
          <span class="badge-dot" />
          PostgreSQL
        </span>
      </header>

      <div class="db-grid">
        <label class="field">
          <span>Server (IP oder Name)</span>
          <input v-model="form.host" type="text" placeholder="192.168.1.10" />
        </label>

        <label class="field">
          <span>Port</span>
          <input v-model.number="form.port" type="number" min="1" max="65535" />
        </label>

        <label class="field">
          <span>Datenbank</span>
          <input v-model="form.database" type="text" placeholder="wartungscockpit" />
        </label>

        <label class="field">
          <span>Benutzer</span>
          <input v-model="form.user" type="text" autocomplete="username" />
        </label>

        <label class="field field--wide">
          <span>Passwort</span>
          <input
            v-model="form.password"
            type="password"
            autocomplete="new-password"
            :placeholder="passwordPlaceholder"
          />
          <small class="field-hint">
            Leer lassen, um das hinterlegte Passwort zu behalten. Gespeichert wird es in
            <code>{{ settingsFile }}</code> – nicht in der Datenbank.
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
          Gespeichert. Die neue Verbindung gilt, sobald das Programm neu gestartet wurde – bis
          dahin läuft alles auf der alten Datenbank weiter.
        </span>
        <button v-if="isDesktop" class="btn btn-sm btn-primary" @click="restartApp">
          Jetzt neu starten
        </button>
      </p>

      <div class="settings-foot">
        <p class="settings-note">
          Diese Angaben gelten für alle Arbeitsplätze, die auf diesen Rechner verbinden. Zieht die
          Datenbank auf einen anderen Server um, reicht hier die neue Adresse – die Anwendung selbst
          muss nicht neu gebaut werden.
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
        </div>
      </div>
    </section>

    <section class="card">
      <header class="card-head">
        <div>
          <h2>Altdaten übernehmen</h2>
          <p>Einmalig beim Wechsel von SQLite</p>
        </div>
      </header>
      <p class="settings-note">
        Die bisherigen Daten aus <code>data/cockpit.db</code> lassen sich mit
        <code>scripts/import-sqlite-to-postgres.py</code> übernehmen. Das Skript schreibt eine
        SQL-Datei, die mit <code>psql</code> eingespielt wird – mehrfaches Ausführen ist
        unschädlich.
      </p>
      <p class="settings-note">
        <Database :size="14" /> Die alte Datei bleibt unverändert liegen und kann als Sicherung
        aufbewahrt werden.
      </p>
    </section>

    <p v-if="!canSave" class="hint-line">
      Nur Administratoren dürfen die Datenbankverbindung ändern.
    </p>
  </template>
</template>
