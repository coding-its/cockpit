<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { Building2, Check, Pencil, Plus, Trash2, X } from 'lucide-vue-next'
import { api, errorMessage } from '../api'
import type { Account } from '../types'

const accounts = ref<Account[]>([])
const loading = ref(true)
const error = ref('')
const notice = ref('')

const draft = ref({ code: '', name: '' })
const creating = ref(false)

const editingId = ref<string | null>(null)
const editDraft = ref({ code: '', name: '' })
const savingId = ref<string | null>(null)

const sorted = computed(() =>
  [...accounts.value].sort((a, b) => a.code.localeCompare(b.code, 'de')),
)

async function load() {
  loading.value = true
  error.value = ''
  try {
    accounts.value = await api.accounts()
  } catch (caught) {
    error.value = errorMessage(caught)
  } finally {
    loading.value = false
  }
}

async function create() {
  const code = draft.value.code.trim()
  const name = draft.value.name.trim()
  if (!code || !name) {
    error.value = 'Bitte Kürzel und Name angeben.'
    return
  }

  creating.value = true
  error.value = ''
  try {
    const account = await api.createAccount({ code, name })
    notice.value = `Konto ${account.code} angelegt.`
    draft.value = { code: '', name: '' }
    await load()
  } catch (caught) {
    error.value = errorMessage(caught)
  } finally {
    creating.value = false
  }
}

function startEdit(account: Account) {
  editingId.value = account.id
  editDraft.value = { code: account.code, name: account.name }
  error.value = ''
}

function cancelEdit() {
  editingId.value = null
}

async function save(account: Account) {
  const code = editDraft.value.code.trim()
  const name = editDraft.value.name.trim()
  if (!code || !name) {
    error.value = 'Bitte Kürzel und Name angeben.'
    return
  }

  savingId.value = account.id
  error.value = ''
  try {
    await api.updateAccount(account.id, { code, name })
    notice.value = `Konto ${code} geändert.`
    editingId.value = null
    await load()
  } catch (caught) {
    error.value = errorMessage(caught)
  } finally {
    savingId.value = null
  }
}

/**
 * Löschen erst nach Rückfrage. Das Backend verweigert das Löschen, solange
 * Kunden dem Konto zugeordnet sind – die Meldung sagt, wie viele es sind.
 */
async function remove(account: Account) {
  const used = account.customer_count + account.web_client_count
  const hint = used
    ? `\n\nAchtung: ${used} Datensätze sind dem Konto zugeordnet.`
    : ''
  if (!window.confirm(`Konto „${account.code} – ${account.name}“ löschen?${hint}`)) return

  error.value = ''
  try {
    await api.deleteAccount(account.id)
    notice.value = `Konto ${account.code} gelöscht.`
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
          <h2>Abrechnungskonten</h2>
          <p>
            Fassen mehrere Datensätze zu einer Rechnung zusammen – etwa wenn eine Firma in
            NinjaOne und in TeamViewer anders heißt.
          </p>
        </div>
        <span class="count-pill">{{ accounts.length }}</span>
      </header>

      <p class="settings-hint">
        Ein Konto bekommt ein Kürzel und einen Namen. Dieses Kürzel wird bei den Kunden
        hinterlegt (Kundenansicht → <strong>Abrechnungskonto</strong>). Alle Kunden mit
        demselben Konto erscheinen in Bericht, Aufstellung und Abrechnung als
        <strong>eine</strong> Position. Ohne Konto rechnet jeder Kunde wie bisher für sich.
      </p>

      <div v-if="error" class="settings-message settings-message--error">{{ error }}</div>
      <div v-if="notice" class="settings-message settings-message--ok">{{ notice }}</div>

      <form class="account-create" @submit.prevent="create">
        <label class="field">
          <span>Kürzel</span>
          <input v-model="draft.code" type="text" maxlength="12" placeholder="AHS" />
        </label>
        <label class="field field--grow">
          <span>Name</span>
          <input v-model="draft.name" type="text" placeholder="Autohaus Schmohl" />
        </label>
        <button class="btn btn-primary" type="submit" :disabled="creating">
          <Plus :size="15" />
          Konto anlegen
        </button>
      </form>

      <div v-if="loading" class="skeleton skeleton--panel" />

      <div v-else-if="!sorted.length" class="empty-state empty-state--compact">
        <Building2 :size="22" />
        <p>Noch kein Konto angelegt. Solange keins existiert, rechnet jeder Kunde einzeln.</p>
      </div>

      <div v-else class="table-wrap">
        <table class="data-table accounts-table">
          <thead>
            <tr>
              <th>Kürzel</th>
              <th>Name</th>
              <th class="num">Kunden</th>
              <th class="num">Web-Kunden</th>
              <th aria-label="Aktionen" />
            </tr>
          </thead>
          <tbody>
            <tr v-for="account in sorted" :key="account.id">
              <template v-if="editingId === account.id">
                <td>
                  <input v-model="editDraft.code" class="inline-input" maxlength="12" />
                </td>
                <td>
                  <input v-model="editDraft.name" class="inline-input inline-input--wide" />
                </td>
                <td class="num">{{ account.customer_count }}</td>
                <td class="num">{{ account.web_client_count }}</td>
                <td class="row-actions">
                  <button
                    class="icon-btn"
                    aria-label="Speichern"
                    :disabled="savingId === account.id"
                    @click="save(account)"
                  >
                    <Check :size="15" />
                  </button>
                  <button class="icon-btn" aria-label="Abbrechen" @click="cancelEdit">
                    <X :size="15" />
                  </button>
                </td>
              </template>
              <template v-else>
                <td><span class="code-badge">{{ account.code }}</span></td>
                <td>{{ account.name }}</td>
                <td class="num">{{ account.customer_count }}</td>
                <td class="num">{{ account.web_client_count }}</td>
                <td class="row-actions">
                  <button class="icon-btn" aria-label="Bearbeiten" @click="startEdit(account)">
                    <Pencil :size="15" />
                  </button>
                  <button
                    class="icon-btn icon-btn--danger"
                    aria-label="Löschen"
                    @click="remove(account)"
                  >
                    <Trash2 :size="15" />
                  </button>
                </td>
              </template>
            </tr>
          </tbody>
        </table>
      </div>
    </section>
  </div>
</template>
