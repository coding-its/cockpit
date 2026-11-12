<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import {
  Building2,
  Check,
  MonitorSmartphone,
  Pencil,
  Plus,
  Server,
  Trash2,
  X,
} from 'lucide-vue-next'
import { api, errorMessage } from '../api'
import type { AccountOverview } from '../types'

const overview = ref<AccountOverview | null>(null)
const loading = ref(true)
const error = ref('')
const notice = ref('')

const draft = ref({ code: '', name: '' })
const creating = ref(false)
const busyAccount = ref<string | null>(null)

const editingId = ref<string | null>(null)
const editDraft = ref({ code: '', name: '' })

const accounts = computed(() => overview.value?.accounts ?? [])
const openCustomers = computed(() => overview.value?.unassigned_customers ?? 0)
const openTeamViewer = computed(() => overview.value?.unassigned_teamviewer ?? 0)

async function load() {
  loading.value = true
  error.value = ''
  try {
    overview.value = await api.accountsOverview()
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
    error.value = 'Bitte Kürzel und Namen angeben.'
    return
  }

  creating.value = true
  error.value = ''
  notice.value = ''
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

function startEdit(accountId: string, code: string, name: string) {
  editingId.value = accountId
  editDraft.value = { code, name }
  error.value = ''
}

async function saveEdit(accountId: string) {
  const code = editDraft.value.code.trim()
  const name = editDraft.value.name.trim()
  if (!code || !name) {
    error.value = 'Bitte Kürzel und Namen angeben.'
    return
  }

  busyAccount.value = accountId
  error.value = ''
  try {
    await api.updateAccount(accountId, { code, name })
    editingId.value = null
    await load()
  } catch (caught) {
    error.value = errorMessage(caught)
  } finally {
    busyAccount.value = null
  }
}

async function remove(accountId: string, code: string, used: number) {
  const hint = used ? `\n\nAchtung: ${used} Datensätze sind dem Konto zugeordnet.` : ''
  if (!window.confirm(`Konto „${code}“ löschen?${hint}`)) return

  error.value = ''
  try {
    await api.deleteAccount(accountId)
    await load()
  } catch (caught) {
    error.value = errorMessage(caught)
  }
}

/** Wechselt den zugeordneten Datensatz eines Kontos. */
async function setCustomer(accountId: string, customerId: string) {
  if (!customerId) return
  busyAccount.value = accountId
  error.value = ''
  try {
    await api.setCustomerAccount(customerId, accountId)
    await load()
  } catch (caught) {
    error.value = errorMessage(caught)
  } finally {
    busyAccount.value = null
  }
}

async function setWebClient(accountId: string, webClientId: string) {
  if (!webClientId) return
  busyAccount.value = accountId
  error.value = ''
  try {
    await api.setWebClientAccount(webClientId, accountId)
    await load()
  } catch (caught) {
    error.value = errorMessage(caught)
  } finally {
    busyAccount.value = null
  }
}

async function setTeamViewerGroup(accountId: string, groupId: string) {
  if (!groupId) return
  busyAccount.value = accountId
  error.value = ''
  try {
    await api.assignGroupToAccount(groupId, accountId)
    await load()
  } catch (caught) {
    error.value = errorMessage(caught)
  } finally {
    busyAccount.value = null
  }
}

function suffix(takenBy: string | null): string {
  return takenBy ? ` · bei ${takenBy}` : ''
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
            Eine Firma, eine Rechnung. Hier werden die Datensätze aus NinjaOne und TeamViewer
            zusammengeführt – der Kontoname gilt dann überall.
          </p>
        </div>
        <span class="count-pill">{{ accounts.length }}</span>
      </header>

      <div v-if="error" class="settings-message settings-message--error">{{ error }}</div>
      <div v-if="notice" class="settings-message settings-message--ok">{{ notice }}</div>

      <p v-if="openCustomers || openTeamViewer" class="settings-hint">
        Noch ohne Konto: <strong>{{ openCustomers }}</strong> Kunde(n) aus NinjaOne,
        <strong>{{ openTeamViewer }}</strong> Gruppe(n) aus TeamViewer.
      </p>

      <form class="account-create" @submit.prevent="create">
        <label class="field">
          <span>Kürzel</span>
          <input v-model="draft.code" type="text" maxlength="12" placeholder="MWK" />
        </label>
        <label class="field field--grow">
          <span>Name</span>
          <input v-model="draft.name" type="text" placeholder="Musterwerk GmbH" />
        </label>
        <button class="btn btn-primary" type="submit" :disabled="creating">
          <Plus :size="15" />
          Konto anlegen
        </button>
      </form>

      <div v-if="loading" class="skeleton skeleton--panel" />

      <div v-else-if="!accounts.length" class="empty-state empty-state--compact">
        <Building2 :size="22" />
        <p>Noch kein Konto angelegt. Solange keins existiert, rechnet jeder Kunde für sich.</p>
      </div>

      <div v-else class="accounts-grid">
        <article v-for="account in accounts" :key="account.account_id" class="account-row">
          <div class="account-code">
            <template v-if="editingId === account.account_id">
              <input v-model="editDraft.code" class="inline-input" maxlength="12" />
            </template>
            <span v-else class="code-badge">{{ account.code }}</span>
          </div>

          <label class="field">
            <span>Name</span>
            <template v-if="editingId === account.account_id">
              <input v-model="editDraft.name" class="inline-input inline-input--wide" />
            </template>
            <strong v-else class="account-name">{{ account.name }}</strong>
          </label>

          <label class="field">
            <span><Server :size="11" /> NinjaOne</span>
            <select
              :value="account.customer_id ?? ''"
              :disabled="busyAccount === account.account_id"
              @change="setCustomer(account.account_id, ($event.target as HTMLSelectElement).value)"
            >
              <option value="">– keiner –</option>
              <option
                v-for="target in overview?.customers ?? []"
                :key="target.id"
                :value="target.id"
              >
                {{ target.name
                }}{{ target.id === account.customer_id ? '' : suffix(target.taken_by) }}
              </option>
            </select>
          </label>

          <label class="field">
            <span><MonitorSmartphone :size="11" /> TeamViewer</span>
            <select
              :value="account.teamviewer_groups[0]?.group_id ?? ''"
              :disabled="busyAccount === account.account_id"
              title="TeamViewer-Gruppe dieses Kontos"
              @change="
                setTeamViewerGroup(account.account_id, ($event.target as HTMLSelectElement).value)
              "
            >
              <option value="">– keine –</option>
              <option
                v-for="target in overview?.teamviewer_groups ?? []"
                :key="target.id"
                :value="target.id"
              >
                {{ target.name }}{{ suffix(target.taken_by) }}
              </option>
            </select>
          </label>

          <div class="account-row-actions">
            <template v-if="editingId === account.account_id">
              <button
                class="icon-btn"
                aria-label="Speichern"
                :disabled="busyAccount === account.account_id"
                @click="saveEdit(account.account_id)"
              >
                <Check :size="15" />
              </button>
              <button class="icon-btn" aria-label="Abbrechen" @click="editingId = null">
                <X :size="15" />
              </button>
            </template>
            <template v-else>
              <button
                class="icon-btn"
                aria-label="Konto bearbeiten"
                @click="startEdit(account.account_id, account.code, account.name)"
              >
                <Pencil :size="15" />
              </button>
              <button
                class="icon-btn icon-btn--danger"
                aria-label="Konto löschen"
                @click="
                  remove(
                    account.account_id,
                    account.code,
                    (account.customer_id ? 1 : 0) +
                      (account.web_client_id ? 1 : 0) +
                      account.teamviewer_groups.length,
                  )
                "
              >
                <Trash2 :size="15" />
              </button>
            </template>
          </div>

          <div class="account-teams">
            <span v-if="account.web_client_id" class="status-pill status-pill--muted">
              Web: {{ account.web_client_name }}
            </span>
            <span
              v-for="group in account.teamviewer_groups"
              :key="group.group_id"
              class="status-pill status-pill--ok"
            >
              TV: {{ group.name }}
            </span>

            <label v-if="(overview?.web_clients.length ?? 0) > 0" class="account-web">
              <span>Web-Kunde (optional)</span>
              <select
                :value="account.web_client_id ?? ''"
                :disabled="busyAccount === account.account_id"
                @change="setWebClient(account.account_id, ($event.target as HTMLSelectElement).value)"
              >
                <option value="">– keiner –</option>
                <option
                  v-for="target in overview?.web_clients ?? []"
                  :key="target.id"
                  :value="target.id"
                >
                  {{ target.name }}{{ suffix(target.taken_by) }}
                </option>
              </select>
            </label>
          </div>
        </article>
      </div>

      <p class="settings-hint">
        Der Kontoname gilt überall: in der Kundenliste, im TeamViewer-Reiter, im Bericht und in
        der Aufstellung. Ein Konto kann einen NinjaOne-Kunden, einen Web-Kunden und mehrere
        TeamViewer-Gruppen enthalten.
      </p>
    </section>
  </div>
</template>
