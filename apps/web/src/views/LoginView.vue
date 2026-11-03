<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { useRouter } from 'vue-router'
import { KeyRound, Loader2, LogIn, ShieldCheck } from 'lucide-vue-next'
import { api, errorMessage } from '../api'
import { useAppStore } from '../stores/app'

const router = useRouter()
const store = useAppStore()

const username = ref('')
const password = ref('')
const busy = ref(false)
const error = ref('')

// Aktuelles Passwort für den erzwungenen Wechsel merken.
const sessionPassword = ref('')
const newPassword = ref('')
const repeatPassword = ref('')
const changeError = ref('')
const changeBusy = ref(false)

async function submit() {
  if (!username.value.trim() || !password.value) {
    error.value = 'Bitte Benutzername und Passwort eingeben.'
    return
  }

  busy.value = true
  error.value = ''
  try {
    const result = await api.login(username.value.trim(), password.value)
    store.setUser(result.user)
    sessionPassword.value = password.value
    password.value = ''

    if (result.must_change_password) {
      newPassword.value = ''
      repeatPassword.value = ''
      return
    }

    const redirect = router.currentRoute.value.query.redirect
    router.push(typeof redirect === 'string' ? redirect : '/')
  } catch (caught) {
    error.value = errorMessage(caught)
  } finally {
    busy.value = false
  }
}

async function saveNewPassword() {
  if (newPassword.value.length < 8) {
    changeError.value = 'Das neue Passwort muss mindestens 8 Zeichen haben.'
    return
  }
  if (newPassword.value !== repeatPassword.value) {
    changeError.value = 'Die Passwörter stimmen nicht überein.'
    return
  }

  changeBusy.value = true
  changeError.value = ''
  try {
    const user = await api.changeOwnPassword(sessionPassword.value, newPassword.value)
    store.setUser(user)
    sessionPassword.value = ''
    store.toast('Passwort geändert. Willkommen.', 'success')
    const redirect = router.currentRoute.value.query.redirect
    router.push(typeof redirect === 'string' ? redirect : '/')
  } catch (caught) {
    changeError.value = errorMessage(caught)
  } finally {
    changeBusy.value = false
  }
}

onMounted(async () => {
  await store.loadSession()
  if (store.user) {
    router.push('/')
  }
})
</script>

<template>
  <div class="login-page">
    <div class="login-card">
      <img class="login-logo" src="/itsolutio-wordmark.png" alt="it.solutio" width="230" />

      <template v-if="!store.mustChangePassword">
        <h1>Anmelden</h1>
        <p class="login-sub">Wartungscockpit · Zugang für das Systemhaus-Team</p>

        <form class="login-form" @submit.prevent="submit">
          <label class="field">
            <span>Benutzername</span>
            <input
              v-model="username"
              type="text"
              autocomplete="username"
              autocapitalize="none"
              spellcheck="false"
              autofocus
              placeholder="Benutzername"
            />
          </label>

          <label class="field">
            <span>Passwort</span>
            <input
              v-model="password"
              type="password"
              autocomplete="current-password"
              placeholder="••••••••"
            />
          </label>

          <p v-if="error" class="form-error">{{ error }}</p>

          <button class="btn btn-primary login-submit" type="submit" :disabled="busy">
            <Loader2 v-if="busy" :size="16" class="spin" />
            <LogIn v-else :size="16" />
            {{ busy ? 'Prüft …' : 'Anmelden' }}
          </button>
        </form>
      </template>

      <template v-else>
        <h1>Passwort ändern</h1>
        <p class="login-sub">
          Hallo {{ store.user?.display_name }}. Bevor es losgeht, braucht dein Konto ein eigenes
          Passwort.
        </p>

        <form class="login-form" @submit.prevent="saveNewPassword">
          <label class="field">
            <span>Neues Passwort (mindestens 8 Zeichen)</span>
            <input v-model="newPassword" type="password" autocomplete="new-password" autofocus />
          </label>

          <label class="field">
            <span>Passwort wiederholen</span>
            <input v-model="repeatPassword" type="password" autocomplete="new-password" />
          </label>

          <p v-if="changeError" class="form-error">{{ changeError }}</p>

          <button class="btn btn-primary login-submit" type="submit" :disabled="changeBusy">
            <KeyRound :size="16" />
            {{ changeBusy ? 'Speichert …' : 'Passwort speichern' }}
          </button>
        </form>
      </template>

      <p class="login-foot">
        <ShieldCheck :size="13" />
        Sitzung läuft 14 Tage. Abmelden beendet sie sofort.
      </p>
    </div>
  </div>
</template>
