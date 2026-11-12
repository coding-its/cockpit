<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { Check, Plus, Tags, X } from 'lucide-vue-next'
import { api, errorMessage } from '../api'
import type { ActivityKind } from '../types'

const kinds = ref<ActivityKind[]>([])
const loading = ref(true)
const error = ref('')
const notice = ref('')

const draft = ref('')
const creating = ref(false)
const busyId = ref<string | null>(null)

const editingId = ref<string | null>(null)
const editName = ref('')

const active = computed(() => kinds.value.filter((kind) => kind.active))
const inactive = computed(() => kinds.value.filter((kind) => !kind.active))

async function load() {
  loading.value = true
  error.value = ''
  try {
    kinds.value = await api.activities(true)
  } catch (caught) {
    error.value = errorMessage(caught)
  } finally {
    loading.value = false
  }
}

async function create() {
  const name = draft.value.trim()
  if (!name) {
    error.value = 'Bitte einen Namen angeben.'
    return
  }

  creating.value = true
  error.value = ''
  notice.value = ''
  try {
    const kind = await api.createActivity(name)
    notice.value = `„${kind.name}“ angelegt.`
    draft.value = ''
    await load()
  } catch (caught) {
    error.value = errorMessage(caught)
  } finally {
    creating.value = false
  }
}

function startEdit(kind: ActivityKind) {
  editingId.value = kind.id
  editName.value = kind.name
  error.value = ''
}

async function saveEdit(kind: ActivityKind) {
  const name = editName.value.trim()
  if (!name) {
    error.value = 'Bitte einen Namen angeben.'
    return
  }

  busyId.value = kind.id
  error.value = ''
  try {
    await api.updateActivity(kind.id, { name })
    editingId.value = null
    await load()
  } catch (caught) {
    error.value = errorMessage(caught)
  } finally {
    busyId.value = null
  }
}

/**
 * Stilllegen statt löschen: Bereits erfasste Zeiten tragen den Namen als
 * Text. Wird die Kategorie gelöscht, verschwindet sie aus dem Menü – die
 * alten Einträge behalten ihren Namen aber.
 */
async function toggle(kind: ActivityKind) {
  busyId.value = kind.id
  error.value = ''
  notice.value = ''
  try {
    await api.updateActivity(kind.id, { active: !kind.active })
    notice.value = kind.active
      ? `„${kind.name}“ stillgelegt – bestehende Zeiten bleiben unverändert.`
      : `„${kind.name}“ wieder verfügbar.`
    await load()
  } catch (caught) {
    error.value = errorMessage(caught)
  } finally {
    busyId.value = null
  }
}

onMounted(load)
</script>

<template>
  <div class="page-stack">
    <section class="card">
      <header class="card-head">
        <div>
          <h2>Leistungsarten</h2>
          <p>
            Die Kategorien, unter denen Zeiten erfasst werden – im TeamViewer-Dialog und unter
            Zeiten auswählbar.
          </p>
        </div>
        <span class="count-pill">{{ active.length }}</span>
      </header>

      <p class="settings-hint">
        Der Name wird beim Erfassen in den Eintrag übernommen. Wird eine Kategorie später
        umbenannt, bleiben bereits erfasste Zeiten unverändert – sie tragen den damaligen Namen.
      </p>

      <div v-if="error" class="settings-message settings-message--error">{{ error }}</div>
      <div v-if="notice" class="settings-message settings-message--ok">{{ notice }}</div>

      <form class="account-create" @submit.prevent="create">
        <label class="field field--grow">
          <span>Neue Leistungsart</span>
          <input v-model="draft" type="text" placeholder="z. B. Schulung" maxlength="80" />
        </label>
        <button class="btn btn-primary" type="submit" :disabled="creating">
          <Plus :size="15" />
          Anlegen
        </button>
      </form>

      <div v-if="loading" class="skeleton skeleton--panel" />

      <template v-else>
        <div v-if="!active.length" class="empty-state empty-state--compact">
          <Tags :size="22" />
          <p>Keine Leistungsart hinterlegt.</p>
        </div>

        <ul v-else class="activity-list">
          <li v-for="kind in active" :key="kind.id" class="activity-row">
            <template v-if="editingId === kind.id">
              <input v-model="editName" class="inline-input inline-input--wide" maxlength="80" />
              <button
                class="icon-btn"
                aria-label="Speichern"
                :disabled="busyId === kind.id"
                @click="saveEdit(kind)"
              >
                <Check :size="15" />
              </button>
              <button class="icon-btn" aria-label="Abbrechen" @click="editingId = null">
                <X :size="15" />
              </button>
            </template>
            <template v-else>
              <span class="activity-name">{{ kind.name }}</span>
              <button class="btn btn-ghost btn--sm" @click="startEdit(kind)">Umbenennen</button>
              <button
                class="btn btn-ghost btn--sm"
                :disabled="busyId === kind.id"
                @click="toggle(kind)"
              >
                Stilllegen
              </button>
            </template>
          </li>
        </ul>

        <details v-if="inactive.length" class="activity-inactive">
          <summary>{{ inactive.length }} stillgelegt</summary>
          <ul class="activity-list">
            <li v-for="kind in inactive" :key="kind.id" class="activity-row is-inactive">
              <span class="activity-name">{{ kind.name }}</span>
              <button
                class="btn btn-ghost btn--sm"
                :disabled="busyId === kind.id"
                @click="toggle(kind)"
              >
                Wieder verfügbar
              </button>
            </li>
          </ul>
        </details>
      </template>
    </section>
  </div>
</template>
