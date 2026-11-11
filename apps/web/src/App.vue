<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
import { RouterLink, RouterView, useRoute, useRouter } from 'vue-router'
import {
  Clock,
  Globe2,
  Inbox,
  LogOut,
  LayoutDashboard,
  FileText,
  Menu,
  MonitorSmartphone,
  PlugZap,
  RefreshCw,
  Settings,
  Square,
  Users,
  Wrench,
  X,
} from 'lucide-vue-next'
import { api, errorMessage } from './api'
import { initials } from './format'
import { formatDateTime, formatTimer, todayLong } from './format'
import { useAppStore } from './stores/app'
import StopCommentModal from './components/StopCommentModal.vue'
import ToastStack from './components/ToastStack.vue'

const route = useRoute()
const router = useRouter()
const store = useAppStore()
const mobileOpen = ref(false)
const buildTime = __BUILD_TIME__
const now = ref(Date.now())
const syncing = ref(false)
let clock: number | undefined

const navItems = computed(() => {
  const items = [
    { to: '/', label: 'Übersicht', icon: LayoutDashboard },
    { to: '/kunden', label: 'NinjaOne', icon: Users },
    { to: '/teamviewer', label: 'TeamViewer', icon: MonitorSmartphone },
    { to: '/websites', label: 'Websites', icon: Globe2 },
    { to: '/wartungen', label: 'Wartungen', icon: Wrench },
    { to: '/einsaetze', label: 'Einsätze', icon: Inbox },
    { to: '/zeiten', label: 'Zeiten', icon: Clock },
    { to: '/bericht', label: 'Bericht', icon: FileText },
    { to: '/integrationen', label: 'Integrationen', icon: PlugZap },
  ]

  if (store.isAdmin) {
    items.push({ to: '/einstellungen', label: 'Einstellungen', icon: Settings })
  }

  return items
})

async function signOut() {
  try {
    await api.logout()
  } finally {
    store.clearSession()
    router.push('/login')
  }
}

const pageTitle = computed(() => (route.meta.title as string | undefined) ?? 'Wartungscockpit')
const isLogin = computed(() => route.name === 'login')

const elapsed = computed(() => {
  if (!store.activeTimer) return 0
  const started = new Date(store.activeTimer.started_at).getTime()
  if (Number.isNaN(started)) return 0
  return Math.max(0, Math.floor((now.value - started) / 1000))
})

async function stopTimer() {
  try {
    await store.stopTimer()
  } catch (error) {
    store.toast(errorMessage(error), 'error')
  }
}

async function syncNinjaOne() {
  syncing.value = true
  try {
    const result = await store.syncNinjaOne()
    store.toast(result.message, 'success')
  } catch (error) {
    store.toast(errorMessage(error), 'error')
  } finally {
    syncing.value = false
  }
}

onMounted(() => {
  clock = window.setInterval(() => {
    now.value = Date.now()
  }, 1000)
})

// Nach jeder Anmeldung Timer und Integrationsstatus nachladen –
// vor dem Login liefern diese Aufrufe noch 401.
watch(
  () => store.user?.id,
  async (id) => {
    if (!id) return
    await Promise.all([store.loadTimer(), store.loadIntegration()])
  },
  { immediate: true },
)

onUnmounted(() => {
  if (clock) window.clearInterval(clock)
})
</script>

<template>
  <RouterView v-if="isLogin" />
  <div v-else class="app-shell">
    <aside class="sidebar" :class="{ 'is-open': mobileOpen }">
      <div class="brand">
        <img
          class="brand-logo"
          src="/itsolutio-wordmark-light.png"
          alt="it.solutio"
          width="190"
          height="37"
        />
        <button class="icon-btn sidebar-close" aria-label="Menü schließen" @click="mobileOpen = false">
          <X :size="18" />
        </button>
      </div>

      <nav class="nav">
        <p class="nav-label">Arbeitsbereich</p>
        <RouterLink
          v-for="item in navItems"
          :key="item.to"
          v-slot="{ href, navigate, isActive }"
          :to="item.to"
          custom
        >
          <a
            :href="href"
            class="nav-item"
            :class="{ 'is-active': isActive }"
            @click="mobileOpen = false; navigate($event)"
          >
            <component :is="item.icon" :size="19" />
            <span>{{ item.label }}</span>
          </a>
        </RouterLink>
      </nav>

      <div class="sidebar-foot">
        <!--
          Zeigt den NinjaOne-Zustand. Der Modus gilt für die ganze
          Installation, nicht für das angemeldete Konto – deshalb nennt der
          Hinweis auch, wer ihn ändern kann.
        -->
        <RouterLink
          v-if="store.isAdmin"
          to="/einstellungen?tab=ninjaone"
          class="demo-card"
          :class="{ 'demo-card--live': store.isLive }"
          :title="store.isLive ? 'NinjaOne-Zugang verwalten' : 'Es sind keine NinjaOne-Zugänge hinterlegt'"
        >
          <span class="pulse-dot" />
          <div>
            <strong>{{ store.isLive ? 'Live-Modus' : 'Testdaten' }}</strong>
            <small>
              {{ store.isLive ? 'NinjaOne · echte Daten' : 'NinjaOne: keine Zugänge hinterlegt' }}
            </small>
          </div>
        </RouterLink>

        <div v-else class="demo-card" :class="{ 'demo-card--live': store.isLive }">
          <span class="pulse-dot" />
          <div>
            <strong>{{ store.isLive ? 'Live-Modus' : 'Testdaten' }}</strong>
            <small>
              {{ store.isLive ? 'NinjaOne · echte Daten' : 'NinjaOne-Zugang fehlt · Administrator fragen' }}
            </small>
          </div>
        </div>

        <!-- Zeigt, welcher Build geladen ist: nach Updates sofort prüfbar. -->
        <p class="build-stamp" :title="`Frontend gebaut am ${formatDateTime(buildTime)}`">
          Stand {{ formatDateTime(buildTime) }}
        </p>
        <div class="user-chip">
          <div class="avatar avatar--small">{{ initials(store.user?.display_name ?? '?') }}</div>
          <div class="user-chip-copy">
            <strong>{{ store.user?.display_name ?? '–' }}</strong>
            <small>{{ store.user?.role_label ?? '' }}</small>
          </div>
          <button class="icon-btn user-logout" title="Abmelden" @click="signOut">
            <LogOut :size="15" />
          </button>
        </div>
      </div>
    </aside>

    <div v-if="mobileOpen" class="sidebar-backdrop" @click="mobileOpen = false" />

    <div class="main">
      <header class="topbar">
        <button class="icon-btn menu-btn" aria-label="Menü öffnen" @click="mobileOpen = true">
          <Menu :size="20" />
        </button>
        <div class="page-heading">
          <h1>{{ pageTitle }}</h1>
          <p>{{ todayLong() }}</p>
        </div>

        <div class="topbar-actions">
          <div v-if="store.activeTimer" class="timer-pill">
            <span class="timer-dot" />
            <div class="timer-meta">
              <small>{{ store.activeTimer.customer_short_code }} · {{ store.activeTimer.activity }}</small>
              <strong>{{ formatTimer(elapsed) }}</strong>
            </div>
            <button class="timer-stop" @click="stopTimer">
              <Square :size="13" />
              Stop
            </button>
          </div>
          <button class="btn btn-ghost sync-btn" :disabled="syncing" @click="syncNinjaOne">
            <RefreshCw :size="16" :class="{ spin: syncing }" />
            NinjaOne
          </button>
        </div>
      </header>

      <main class="content">
        <RouterView v-slot="{ Component }">
          <Transition name="page" mode="out-in">
            <component :is="Component" />
          </Transition>
        </RouterView>
      </main>
    </div>

    <ToastStack />
    <StopCommentModal />
  </div>
</template>
