<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useRouter } from 'vue-router'
import {
  Check,
  KeyRound,
  LogOut,
  Plus,
  RefreshCw,
  ShieldAlert,
  UserCog,
  UserRound,
} from 'lucide-vue-next'
import { api, errorMessage } from '../api'
import type { AppUser, CreateUserPayload, UserRole } from '../types'
import { formatDateTime, initials, relativeTime } from '../format'
import Modal from '../components/Modal.vue'
import { useAppStore } from '../stores/app'

const router = useRouter()
const store = useAppStore()
const users = ref<AppUser[]>([])
const loading = ref(true)
const error = ref('')
const modalOpen = ref(false)
const saving = ref(false)
const formError = ref('')
const resetOpen = ref(false)
const resetTarget = ref<AppUser | null>(null)
const resetPassword = ref('')
const resetError = ref('')
const resetSaving = ref(false)

const form = ref<CreateUserPayload>({
  username: '',
  display_name: '',
  password: '',
  role: 'technician',
})

const roles = [
  { value: 'admin', label: 'Administrator', hint: 'Alles, inklusive Benutzerverwaltung' },
  { value: 'technician', label: 'Techniker', hint: 'Arbeiten ausführen, Zeiten erfassen' },
  { value: 'viewer', label: 'Nur Lesen', hint: 'Sieht alles, ändert nichts' },
]

const activeCount = computed(() => users.value.filter((user) => user.active).length)

function roleLabel(role: string): string {
  return roles.find((entry) => entry.value === role)?.label ?? role
}

function roleHint(role: string): string {
  return roles.find((entry) => entry.value === role)?.hint ?? ''
}

async function load() {
  loading.value = true
  error.value = ''
  try {
    users.value = await api.users()
  } catch (caught) {
    error.value = errorMessage(caught)
  } finally {
    loading.value = false
  }
}

function openModal() {
  form.value = { username: '', display_name: '', password: '', role: 'technician' }
  formError.value = ''
  modalOpen.value = true
}

async function save() {
  if (!form.value.username.trim() || !form.value.display_name.trim()) {
    formError.value = 'Bitte Benutzername und Anzeigename angeben.'
    return
  }
  if (form.value.password.length < 8) {
    formError.value = 'Das Passwort muss mindestens 8 Zeichen haben.'
    return
  }

  saving.value = true
  formError.value = ''
  try {
    await api.createUser(form.value)
    modalOpen.value = false
    store.toast('Benutzer angelegt. Beim ersten Login muss das Passwort geändert werden.', 'success')
    await load()
  } catch (caught) {
    formError.value = errorMessage(caught)
  } finally {
    saving.value = false
  }
}

async function toggleActive(user: AppUser) {
  try {
    await api.updateUser(user.id, { active: !user.active })
    store.toast(user.active ? `${user.display_name} deaktiviert.` : `${user.display_name} aktiviert.`, 'success')
    await load()
  } catch (caught) {
    store.toast(errorMessage(caught), 'error')
  }
}

async function changeRole(user: AppUser, role: UserRole) {
  try {
    await api.updateUser(user.id, { role })
    store.toast(`Rolle von ${user.display_name}: ${roleLabel(role)}`, 'success')
    await load()
  } catch (caught) {
    store.toast(errorMessage(caught), 'error')
    await load()
  }
}

function openReset(user: AppUser) {
  resetTarget.value = user
  resetPassword.value = ''
  resetError.value = ''
  resetOpen.value = true
}

async function saveReset() {
  if (!resetTarget.value) return
  if (resetPassword.value.length < 8) {
    resetError.value = 'Das Passwort muss mindestens 8 Zeichen haben.'
    return
  }
  resetSaving.value = true
  resetError.value = ''
  try {
    await api.resetUserPassword(resetTarget.value.id, resetPassword.value)
    resetOpen.value = false
    store.toast('Passwort gesetzt. Beim nächsten Login muss es geändert werden.', 'success')
    await load()
  } catch (caught) {
    resetError.value = errorMessage(caught)
  } finally {
    resetSaving.value = false
  }
}

async function logout() {
  try {
    await api.logout()
  } finally {
    store.clearSession()
    router.push('/login')
  }
}

onMounted(load)
</script>

<template>
  <div class="settings-panel">
    <div v-if="loading" class="skeleton skeleton--panel" />
    <div v-else-if="error" class="card empty-state">
      <ShieldAlert :size="26" />
      <p>{{ error }}</p>
      <button class="btn btn-primary" @click="load">Erneut versuchen</button>
    </div>

    <template v-else>
      <div class="card">
        <header class="card-head">
          <div>
            <h2>Benutzer</h2>
            <p>Wer das Cockpit benutzt und was er darf</p>
          </div>
          <button v-if="store.isAdmin" class="btn btn-primary" @click="openModal">
            <Plus :size="15" />
            Benutzer anlegen
          </button>
        </header>

        <table class="data-table users-table">
          <thead>
            <tr>
              <th>Benutzer</th>
              <th>Rolle</th>
              <th>Status</th>
              <th>Letzte Anmeldung</th>
              <th aria-label="Aktionen" />
            </tr>
          </thead>
          <tbody>
            <tr v-for="user in users" :key="user.id">
              <td>
                <div class="entity-cell">
                  <div class="avatar" :class="{ 'avatar--muted': !user.active }">
                    {{ initials(user.display_name) }}
                  </div>
                  <div>
                    <strong>{{ user.display_name }}</strong>
                    <small>{{ user.username }}<template v-if="user.id === store.user?.id"> · das bist du</template></small>
                  </div>
                </div>
              </td>
              <td>
                <select
                  class="role-select"
                  :value="user.role"
                  :disabled="user.id === store.user?.id"
                  @change="changeRole(user, ($event.target as HTMLSelectElement).value as UserRole)"
                >
                  <option v-for="role in roles" :key="role.value" :value="role.value">
                    {{ role.label }}
                  </option>
                </select>
                <small class="role-hint">{{ roleHint(user.role) }}</small>
              </td>
              <td>
                <span class="status-pill" :class="user.active ? 'status-pill--ok' : 'status-pill--muted'">
                  <span class="badge-dot" />
                  {{ user.active ? 'aktiv' : 'deaktiviert' }}
                </span>
                <span v-if="user.must_change_password" class="status-pill status-pill--warning">Passwortwechsel offen</span>
              </td>
              <td>
                <div class="stacked-cell">
                  <span>{{ user.last_login_at ? formatDateTime(user.last_login_at) : 'nie' }}</span>
                  <small v-if="user.last_login_at">{{ relativeTime(user.last_login_at) }}</small>
                </div>
              </td>
              <td class="row-actions">
                <button class="btn btn-ghost btn-sm" @click="openReset(user)">
                  <KeyRound :size="13" />
                  Passwort
                </button>
                <button
                  class="btn btn-ghost btn-sm"
                  :disabled="user.id === store.user?.id"
                  @click="toggleActive(user)"
                >
                  {{ user.active ? 'Deaktivieren' : 'Aktivieren' }}
                </button>
              </td>
            </tr>
          </tbody>
        </table>
      </div>

      <section class="info-strip">
        <div class="info-strip-icon"><UserCog :size="18" /></div>
        <p>
          <strong>{{ activeCount }} aktive Benutzer.</strong>
          Neue Konten müssen bei der ersten Anmeldung ein eigenes Passwort setzen. Wer deaktiviert
          wird, fliegt sofort aus allen Sitzungen. Der letzte aktive Administrator kann nicht
          herabgestuft werden.
        </p>
        <button class="btn btn-ghost btn-sm" @click="logout">
          <LogOut :size="14" />
          Abmelden
        </button>
      </section>
    </template>

    <Modal
      :open="modalOpen"
      title="Benutzer anlegen"
      subtitle="Das Startpasswort muss beim ersten Login geändert werden."
      @close="modalOpen = false"
    >
      <form class="form-grid" @submit.prevent="save">
        <label class="field field--full">
          <span>Anzeigename</span>
          <input v-model="form.display_name" type="text" placeholder="z. B. Max Mustermann" required />
        </label>

        <label class="field field--full">
          <span>Benutzername</span>
          <input v-model="form.username" type="text" placeholder="z. B. m.mustermann" required />
        </label>

        <label class="field field--full">
          <span>Startpasswort (mindestens 8 Zeichen)</span>
          <input v-model="form.password" type="text" autocomplete="new-password" required />
        </label>

        <label class="field field--full">
          <span>Rolle</span>
          <select v-model="form.role">
            <option v-for="role in roles" :key="role.value" :value="role.value">
              {{ role.label }} – {{ role.hint }}
            </option>
          </select>
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

    <Modal
      :open="resetOpen"
      title="Passwort setzen"
      :subtitle="resetTarget ? `Neues Passwort für ${resetTarget.display_name}` : ''"
      @close="resetOpen = false"
    >
      <form class="form-grid" @submit.prevent="saveReset">
        <label class="field field--full">
          <span>Neues Passwort (mindestens 8 Zeichen)</span>
          <input v-model="resetPassword" type="text" autocomplete="new-password" required />
        </label>
        <p class="form-hint field--full">
          Alle laufenden Sitzungen dieses Kontos werden beendet. Beim nächsten Login muss das
          Passwort erneut geändert werden.
        </p>
        <p v-if="resetError" class="form-error field--full">{{ resetError }}</p>
      </form>

      <template #footer>
        <button class="btn btn-ghost" type="button" @click="resetOpen = false">Abbrechen</button>
        <button class="btn btn-primary" type="button" :disabled="resetSaving" @click="saveReset">
          {{ resetSaving ? 'Setzt …' : 'Passwort setzen' }}
        </button>
      </template>
    </Modal>
  </div>
</template>
