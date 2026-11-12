<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { Check, KeyRound, RefreshCw, Save, ShieldAlert, Trash2 } from 'lucide-vue-next'
import { api, errorMessage } from '../api'
import { useAppStore } from '../stores/app'

const store = useAppStore()

const loading = ref(true)
const saving = ref(false)
const testing = ref(false)
const error = ref('')
const result = ref<{ ok: boolean; text: string } | null>(null)

const configured = ref(false)
const source = ref<string | null>(null)
const draft = ref('')

const canWrite = computed(() => store.isAdmin)

const placeholder = computed(() =>
  configured.value ? 'unverändert lassen' : 'Script-Token einfügen',
)

async function load() {
  loading.value = true
  error.value = ''
  try {
    const data = await api.teamviewerSettings()
    configured.value = data.configured
    source.value = data.source
  } catch (caught) {
    error.value = errorMessage(caught)
  } finally {
    loading.value = false
  }
}

/**
 * Speichert nur, wenn ein Token eingegeben wurde – ein leeres Feld lässt den
 * hinterlegten stehen. So kann man den Status prüfen, ohne den Token zu
 * kennen.
 */
async function save() {
  saving.value = true
  error.value = ''
  result.value = null
  try {
    const response = await api.updateTeamViewerSettings({ token: draft.value.trim() })
    draft.value = ''
    if (response.report) {
      result.value = { ok: true, text: response.report }
    }
    await load()
  } catch (caught) {
    error.value = errorMessage(caught)
  } finally {
    saving.value = false
  }
}

/** Prüft die Eingabe, ohne sie zu speichern. Ohne Eingabe: den hinterlegten. */
async function test() {
  testing.value = true
  error.value = ''
  result.value = null
  try {
    const response = await api.testTeamViewerSettings(draft.value.trim())
    result.value = { ok: true, text: response.report }
  } catch (caught) {
    error.value = errorMessage(caught)
  } finally {
    testing.value = false
  }
}

async function remove() {
  if (!window.confirm('TeamViewer-Zugang entfernen? Der Abgleich ist danach nicht mehr möglich.')) {
    return
  }

  error.value = ''
  result.value = null
  try {
    await api.updateTeamViewerSettings({ clear: true })
    draft.value = ''
    await load()
  } catch (caught) {
    error.value = errorMessage(caught)
  }
}

onMounted(load)
</script>

<template>
  <div class="page-stack">
    <section class="card">
      <header class="card-head">
        <div>
          <h2>TeamViewer-Zugang</h2>
          <p>
            Script-Token aus dem TeamViewer Management Console. Er wird hier hinterlegt und
            nicht mehr angezeigt.
          </p>
        </div>
        <span
          class="status-pill"
          :class="configured ? 'status-pill--ok' : 'status-pill--warning'"
        >
          <Check v-if="configured" :size="13" />
          <ShieldAlert v-else :size="13" />
          {{ configured ? 'hinterlegt' : 'nicht hinterlegt' }}
        </span>
      </header>

      <p class="settings-hint">
        So anlegen: im Management Console oben rechts auf den Namen →
        <strong>Profil bearbeiten</strong> → <strong>Apps</strong> →
        <strong>Skript-Token erstellen</strong>. Nötig sind lesende Rechte in
        <strong>Verbindungsprotokollierung</strong> (Verbindungsprotokolle lesen) und
        <strong>Computer &amp; Kontakte</strong> (Einträge anzeigen). Ein Token lässt sich
        nachträglich nicht ändern – nur löschen und neu anlegen.
      </p>

      <p v-if="configured && source" class="settings-hint">
        Quelle: <code>{{ source }}</code>
      </p>
      <p v-if="configured && source?.startsWith('Umgebungsvariable')" class="report-note report-note--warn">
        Der Token kommt aus einer Umgebungsvariable. Sie hat Vorrang – ein hier eingetragener
        Token hätte keine Wirkung, solange die Variable gesetzt ist.
      </p>

      <div v-if="error" class="settings-message settings-message--error">{{ error }}</div>
      <div v-if="result" class="settings-message settings-message--ok">{{ result.text }}</div>

      <div v-if="loading" class="skeleton skeleton--panel" />

      <template v-else>
        <label class="field field--wide">
          <span>{{ configured ? 'Neuen Token hinterlegen' : 'Token' }}</span>
          <input
            v-model="draft"
            type="password"
            autocomplete="off"
            spellcheck="false"
            :placeholder="placeholder"
            :disabled="!canWrite"
          />
        </label>

        <div class="toolbar-actions settings-actions">
          <button class="btn btn-ghost" :disabled="testing || (!draft.trim() && !configured)" @click="test">
            <RefreshCw :size="15" />
            {{ testing ? 'Wird geprüft …' : 'Zugang testen' }}
          </button>
          <button
            class="btn btn-primary"
            :disabled="saving || !draft.trim() || !canWrite"
            @click="save"
          >
            <Save :size="15" />
            {{ saving ? 'Wird gespeichert …' : 'Speichern' }}
          </button>
          <button v-if="configured" class="btn btn-ghost" :disabled="!canWrite" @click="remove">
            <Trash2 :size="15" />
            Entfernen
          </button>
        </div>

        <p class="settings-hint">
          Der Token gilt für <strong>diesen</strong> Benutzer: Sichtbar sind nur die
          Verbindungen, die mit seinem TeamViewer-Konto protokolliert wurden. Für jeden Techniker
          braucht es einen eigenen Token. Änderungen wirken beim nächsten Abgleich – ein Neustart
          ist nicht nötig.
        </p>

        <p v-if="!canWrite" class="settings-hint">
          <KeyRound :size="13" /> Nur Administratoren dürfen den Zugang ändern.
        </p>
      </template>
    </section>
  </div>
</template>
