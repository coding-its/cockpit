<script setup lang="ts">
import { onMounted, ref } from 'vue'
import {
  AlertTriangle,
  CheckCircle2,
  Database,
  ExternalLink,
  Globe2,
  Inbox,
  KeyRound,
  Link2,
  RefreshCw,
  ShieldCheck,
  Terminal,
} from 'lucide-vue-next'
import { api, errorMessage } from '../api'
import type { AuditEvent, IntegrationStatus } from '../types'
import { formatDateTime, relativeTime } from '../format'
import { TIX_URL } from '../links'
import { useAppStore } from '../stores/app'

const store = useAppStore()
const status = ref<IntegrationStatus | null>(null)
const audit = ref<AuditEvent[]>([])
const loading = ref(true)
const syncing = ref(false)
const monitoringBusy = ref(false)
const connecting = ref(false)
const error = ref('')

async function load() {
  loading.value = true
  error.value = ''
  try {
    const [integration, events] = await Promise.all([api.integrations(), api.audit()])
    status.value = integration
    audit.value = events
  } catch (caught) {
    error.value = errorMessage(caught)
  } finally {
    loading.value = false
  }
}

async function connectNinjaOne() {
  connecting.value = true
  try {
    const { url } = await api.ninjaoneAuthStart()
    window.open(url, '_blank', 'noopener')
    store.toast('NinjaOne-Zustimmung im neuen Fenster abschließen.', 'info')
  } catch (caught) {
    store.toast(errorMessage(caught), 'error')
  } finally {
    connecting.value = false
  }
}

async function sync() {
  syncing.value = true
  try {
    const result = await api.sync()
    store.toast(`${result.message} Modus: ${result.mode}.`, 'success')
    await load()
  } catch (caught) {
    store.toast(errorMessage(caught), 'error')
  } finally {
    syncing.value = false
  }
}

async function refreshMonitoring() {
  monitoringBusy.value = true
  try {
    const result = await api.syncMonitoring()
    store.toast(result.message, 'success')
    await load()
  } catch (caught) {
    store.toast(errorMessage(caught), 'error')
  } finally {
    monitoringBusy.value = false
  }
}

onMounted(load)
</script>

<template>
  <div class="page-stack">
    <div v-if="loading" class="skeleton-stack">
      <div class="skeleton skeleton--hero" />
      <div class="skeleton skeleton--panel" />
    </div>

    <div v-else-if="error" class="card empty-state">
      <AlertTriangle :size="26" />
      <p>{{ error }}</p>
      <button class="btn btn-primary" @click="load">Erneut versuchen</button>
    </div>

    <template v-else-if="status">
      <section class="hero hero--compact">
        <div>
          <p class="eyebrow">Schnittstellen</p>
          <h2>NinjaOne und WordPress verbinden</h2>
          <p class="hero-sub">
            NinjaOne liefert Kunden, Geräte und Ausführungsjobs. Der WordPress-Agent übernimmt
            Inventar, Plugin- und Core-Updates. Beide Wege laufen über nachvollziehbare Jobs.
          </p>
        </div>
        <button class="btn btn-primary" :disabled="syncing" @click="sync">
          <RefreshCw :size="16" :class="{ spin: syncing }" />
          {{ syncing ? 'Synchronisiert …' : 'NinjaOne synchronisieren' }}
        </button>
      </section>

      <div class="integration-grid">
        <section class="card integration-card">
          <header class="integration-head">
            <div class="integration-icon integration-icon--teal"><Link2 :size="20" /></div>
            <div>
              <h2>NinjaOne</h2>
              <p>Public API v2 · OAuth 2.0 Client Credentials</p>
            </div>
            <span
              class="status-pill"
              :class="status.ninjaone_mode === 'live' ? 'status-pill--ok' : 'status-pill--warning'"
            >
              <span class="badge-dot" />
              {{ status.ninjaone_mode === 'live' ? 'Live' : 'Mock-Modus' }}
            </span>
          </header>

          <div class="integration-rows">
            <div class="integration-row">
              <span>Basis-URL</span>
              <code>{{ status.ninjaone_base_url }}</code>
            </div>
            <div class="integration-row">
              <span>Letzter Sync</span>
              <strong>{{ status.last_sync ? formatDateTime(status.last_sync) : 'Noch nicht' }}</strong>
            </div>
            <div class="integration-row">
              <span>Scopes</span>
              <code>monitoring management</code>
            </div>
            <div class="integration-row">
              <span>Webhook</span>
              <strong>{{ status.webhook_slots }}</strong>
            </div>
            <div class="integration-row">
              <span>Patch &amp; Antivirus</span>
              <strong>
                <template v-if="status.monitoring_checked_at">
                  {{ status.monitoring_patch_devices }} Geräte mit Patchdaten ·
                  {{ status.monitoring_pending_os }} OS- und
                  {{ status.monitoring_pending_software }} Software-Patches offen ·
                  {{ status.monitoring_av_warnings }} AV zu prüfen
                </template>
                <template v-else>noch nicht abgerufen</template>
              </strong>
            </div>
            <div class="integration-row">
              <span>Credentials</span>
              <strong>Über Umgebungsvariablen, nie im Frontend</strong>
            </div>
            <div class="integration-row">
              <span>Benutzerkontext</span>
              <strong>
                {{ status.user_connected ? 'verbunden' : status.user_app_configured ? 'App konfiguriert, Benutzer fehlt' : 'nicht konfiguriert' }}
              </strong>
            </div>
          </div>

          <div v-if="!status.user_connected" class="connect-block">
            <div class="connect-copy">
              <strong>Scripts brauchen einen Benutzer</strong>
              <p>
                NinjaOne lehnt <code>script/run</code> mit einem Machine-Token ab
                (<code>403 user_context_required</code>). Patches funktionieren, Scripts nicht.
                Dafür braucht es eine zweite App vom Typ <em>Webanwendung</em> und einmalig eine
                Zustimmung durch einen NinjaOne-Administrator.
              </p>
            </div>
            <button
              class="btn btn-primary btn-sm"
              :disabled="!status.user_app_configured || connecting"
              @click="connectNinjaOne"
            >
              <Link2 :size="14" />
              {{ connecting ? 'Öffnet …' : 'Benutzer verbinden' }}
            </button>
          </div>

          <p v-else class="integration-note">
            <CheckCircle2 :size="15" />
            <span>
              Benutzerkontext ist hinterlegt. Script-Aufgaben werden mit dem Token des verbundenen
              Benutzers ausgeführt und im Audit-Trail diesem Benutzer zugeordnet.
            </span>
          </p>

          <div class="monitoring-strip">
            <div>
              <strong>Patch- und Antivirusstand</strong>
              <p v-if="status.monitoring_checked_at">
                Zuletzt abgerufen {{ relativeTime(status.monitoring_checked_at) }}. Ein Aufruf pro
                Kategorie für den ganzen Bestand.
              </p>
              <p v-else>Noch nie abgerufen – die Geräte zeigen „unbekannt", bis der Abgleich läuft.</p>
            </div>
            <button class="btn btn-ghost btn-sm" :disabled="monitoringBusy" @click="refreshMonitoring">
              <RefreshCw :size="14" :class="{ spin: monitoringBusy }" />
              {{ monitoringBusy ? 'Holt …' : 'Jetzt abrufen' }}
            </button>
          </div>

          <p v-if="!status.user_connected" class="integration-note integration-note--warning">
            <AlertTriangle :size="15" />
            <span>
              Ohne verbundenen Benutzer bleiben Script-Aufgaben gesperrt. Patches und Inventar laufen
              weiterhin über das Machine-Token.
            </span>
          </p>
        </section>

        <section class="card integration-card">
          <header class="integration-head">
            <div class="integration-icon integration-icon--blue"><Globe2 :size="20" /></div>
            <div>
              <h2>WordPress-Agent</h2>
              <p>Mu-Plugin · REST unter <code>/wp-json/maintenance/v1</code></p>
            </div>
            <span
              class="status-pill"
              :class="status.wp_agent_configured ? 'status-pill--ok' : 'status-pill--warning'"
            >
              <span class="badge-dot" />
              {{ status.wp_agent_configured ? 'Secret gesetzt' : 'Secret fehlt' }}
            </span>
          </header>

          <div class="code-block">
            <div><span>GET</span> /health</div>
            <div><span>GET</span> /inventory</div>
            <div><span>POST</span> /updates/plugins</div>
            <div><span>POST</span> /updates/core</div>
          </div>

          <div class="integration-rows">
            <div class="integration-row">
              <span>Auth</span>
              <strong>Application Password + HMAC + IP-Allowlist</strong>
            </div>
            <div class="integration-row">
              <span>Rechte</span>
              <code>update_plugins</code>
            </div>
            <div class="integration-row">
              <span>Fallback</span>
              <strong>WP-CLI über SSH, nur feste Befehle</strong>
            </div>
          </div>

          <p class="integration-note integration-note--warning">
            <AlertTriangle :size="15" />
            <span>
              Vor jedem Update muss ein Backup existieren. Der Agent prüft nach dem Update die
              Erreichbarkeit und meldet Fehler, statt still weiterzulaufen.
            </span>
          </p>
        </section>
      </div>

      <section class="card integration-card">
        <header class="integration-head">
          <div class="integration-icon integration-icon--violet"><Inbox :size="20" /></div>
          <div>
            <h2>tix Ticketsystem</h2>
            <p>Externes Ticketsystem von consultingME</p>
          </div>
          <span class="status-pill status-pill--muted">
            <span class="badge-dot" />
            nur verlinkt
          </span>
        </header>

        <p class="integration-note">
          <ExternalLink :size="15" />
          <span>
            tix hat keine dokumentierte Schnittstelle – das System wird als Nebenprojekt gepflegt.
            Das Cockpit verlinkt es deshalb nur; Einsätze werden hier erfasst, Tickets dort.
          </span>
        </p>

        <div class="integration-actions">
          <a class="btn btn-ghost" :href="TIX_URL" target="_blank" rel="noreferrer">
            <ExternalLink :size="15" />
            tix öffnen
          </a>
          <code class="integration-url">{{ TIX_URL }}</code>
        </div>
      </section>

      <div class="risk-grid">
        <article class="card risk-card">
          <div class="risk-icon risk-icon--amber"><AlertTriangle :size="18" /></div>
          <h3>Ein Webhook pro Tenant</h3>
          <p>
            NinjaOne erlaubt genau eine Webhook-Konfiguration. Nutzt sie bereits ein anderes Tool,
            muss das Cockpit pollen oder den bestehenden Empfänger ablösen.
          </p>
        </article>
        <article class="card risk-card">
          <div class="risk-icon risk-icon--amber"><KeyRound :size="18" /></div>
          <h3>Script-Token verifizieren</h3>
          <p>
            <code>script/run</code> kann mit einem reinen Machine-Token abgelehnt werden. Der PoC
            muss klären, ob ein Benutzerkontext-Token nötig ist.
          </p>
        </article>
        <article class="card risk-card">
          <div class="risk-icon risk-icon--teal"><ShieldCheck :size="18" /></div>
          <h3>Sicherheit vor Automation</h3>
          <p>
            Credentials verschlüsselt speichern, jede Ausführung auditieren, risikoreiche Updates
            nur nach Freigabe und mit verifiziertem Backup.
          </p>
        </article>
      </div>

      <section class="card">
        <header class="card-head">
          <div>
            <h2>Audit-Trail</h2>
            <p>Jede Ausführung, jeder Sync und jede Credential-Änderung wird protokolliert</p>
          </div>
          <span class="count-pill">{{ audit.length }}</span>
        </header>

        <div class="table-wrap table-scroll--eight">
          <table class="data-table data-table--compact">
            <thead>
              <tr>
                <th>Zeit</th>
                <th>Akteur</th>
                <th>Aktion</th>
                <th>Detail</th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="event in audit" :key="event.id">
                <td>
                  <div class="stacked-cell">
                    <span>{{ formatDateTime(event.created_at) }}</span>
                    <small>{{ relativeTime(event.created_at) }}</small>
                  </div>
                </td>
                <td>{{ event.actor }}</td>
                <td>{{ event.action }}</td>
                <td class="cell-wrap">{{ event.detail ?? '–' }}</td>
              </tr>
            </tbody>
          </table>
        </div>
      </section>

      <section class="card roadmap-card">
        <header class="card-head">
          <div>
            <h2>Nächste Ausbaustufen</h2>
            <p>Was der Prototyp bewusst noch nicht enthält</p>
          </div>
        </header>
        <div class="roadmap-list">
          <div class="roadmap-item">
            <span class="roadmap-phase">Phase 1</span>
            <div>
              <strong>Credential-Verwaltung und Login</strong>
              <p>Verschlüsselte WordPress-Zugangsdaten, Rollen und Session-Login.</p>
            </div>
          </div>
          <div class="roadmap-item">
            <span class="roadmap-phase">Phase 2</span>
            <div>
              <strong>Geplante Läufe und Webhooks</strong>
              <p>Wartungsfenster automatisch erzeugen, NinjaOne-Ereignisse empfangen, Reports als PDF.</p>
            </div>
          </div>
          <div class="roadmap-item">
            <span class="roadmap-phase">Phase 3</span>
            <div>
              <strong>Abrechnung und Kundenportal</strong>
              <p>sevDesk-/Lexware-Adapter, signierte Kundenlinks und PWA-Feinschliff.</p>
            </div>
          </div>
        </div>
      </section>

      <section class="info-strip">
        <div class="info-strip-icon"><Database :size="18" /></div>
        <p>
          <strong>PoC-Datenbank:</strong> SQLite für den Zero-Setup-Start. Das Schema ist
          Postgres-kompatibel; Phase 1 wechselt auf PostgreSQL 16 und Docker Compose.
        </p>
      </section>
    </template>
  </div>
</template>
