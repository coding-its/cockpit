<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue'
import { RouterLink, useRoute } from 'vue-router'
import {
  CalendarDays,
  CheckCircle2,
  ChevronRight,
  Clock3,
  Cpu,
  Info,
  ShieldAlert,
  UserRound,
} from 'lucide-vue-next'
import { api, errorMessage } from '../api'
import type { Job, RunDetail, Task } from '../types'
import { formatDateTime, formatDuration, priorityLabel, runKindLabel } from '../format'
import { useAppStore } from '../stores/app'
import ProgressBar from '../components/ProgressBar.vue'
import StatusBadge from '../components/StatusBadge.vue'
import TaskItem from '../components/TaskItem.vue'

const route = useRoute()
const store = useAppStore()
const detail = ref<RunDetail | null>(null)
const loading = ref(true)
const error = ref('')
const jobs = ref<Record<string, Job>>({})
const executing = ref<Record<string, boolean>>({})
let pollHandle: number | undefined

const isTicket = computed(() => detail.value?.run.kind === 'ticket')

const completed = computed(
  () => detail.value?.tasks.filter((task) => task.status === 'done').length ?? 0,
)
const total = computed(() => detail.value?.tasks.length ?? 0)
const progress = computed(() => (total.value ? (completed.value / total.value) * 100 : 0))
const taskTitles = computed(() =>
  Object.fromEntries((detail.value?.tasks ?? []).map((task) => [task.id, task.title])),
)


/// Wandelt den gespeicherten Antworttext in eine lesbare Zeile.
function readableResult(raw?: string | null): string {
  if (!raw) return '–'

  const trimmed = raw.trim()
  if (!trimmed.startsWith('{')) return trimmed

  try {
    const data = JSON.parse(trimmed) as Record<string, unknown>

    if (typeof data.summary === 'string' && data.summary.trim()) return data.summary

    const activity = data.activity as Record<string, unknown> | null | undefined
    if (activity && typeof activity.message === 'string' && activity.message.trim()) {
      const message = activity.message.trim().replace(/\s+/g, ' ')
      return message.length > 160 ? `${message.slice(0, 160)}…` : message
    }

    if (typeof data.status === 'string' && data.status) return data.status
    if (typeof data.error === 'string' && data.error) return data.error
    if (typeof data.http_status === 'number') {
      return `NinjaOne antwortete mit HTTP ${data.http_status}`
    }
    return trimmed.length > 140 ? `${trimmed.slice(0, 140)}…` : trimmed
  } catch {
    return trimmed
  }
}

/// Zeigt den Rohdaten-Hinweis nur, wenn tatsächlich mehr als ein Satz dahintersteckt.
function hasRawDetail(raw?: string | null): boolean {
  if (!raw) return false
  const trimmed = raw.trim()
  return trimmed.startsWith('{') && trimmed.length > 60
}

async function load(initial = false) {
  if (initial) loading.value = true
  try {
    detail.value = await api.run(route.params.id as string)
    route.meta.title = detail.value.run.kind === 'ticket' ? 'Einsatz' : 'Wartungslauf'
    error.value = ''
  } catch (caught) {
    if (!detail.value) error.value = errorMessage(caught)
  } finally {
    loading.value = false
  }
}

async function execute(task: Task) {
  if (executing.value[task.id]) return
  executing.value = { ...executing.value, [task.id]: true }

  try {
    const result = await api.executeTask(task.id)
    jobs.value = {
      ...jobs.value,
      [task.id]: {
        id: result.job_id,
        task_id: task.id,
        kind: task.kind,
        status: 'queued',
        progress: 0,
        message: 'Aufgabe wird vorbereitet',
        payload: null,
        created_at: new Date().toISOString(),
        updated_at: new Date().toISOString(),
      },
    }
    await load()
    store.toast('Aufgabe gestartet.', 'info')
  } catch (caught) {
    executing.value = { ...executing.value, [task.id]: false }
    store.toast(errorMessage(caught), 'error')
  }
}

async function approve(task: Task) {
  try {
    await api.approveTask(task.id)
    await load()
    store.toast('Aufgabe freigegeben. Sie kann jetzt ausgeführt werden.', 'success')
  } catch (caught) {
    store.toast(errorMessage(caught), 'error')
  }
}

async function pollJobs() {
  const pending = Object.entries(jobs.value).filter(
    ([, job]) => job.status === 'queued' || job.status === 'running',
  )
  if (!pending.length) return

  for (const [taskId, job] of pending) {
    try {
      const fresh = await api.job(job.id)
      jobs.value = { ...jobs.value, [taskId]: fresh }

      if (fresh.status === 'succeeded' || fresh.status === 'failed') {
        executing.value = { ...executing.value, [taskId]: false }
        await load()
        store.toast(
          fresh.status === 'succeeded' ? 'Aufgabe abgeschlossen.' : 'Aufgabe fehlgeschlagen.',
          fresh.status === 'succeeded' ? 'success' : 'error',
        )
      }
    } catch {
      // Einzelner Poll-Fehler wird beim nächsten Durchlauf erneut versucht.
    }
  }
}

onMounted(() => {
  load(true)
  pollHandle = window.setInterval(pollJobs, 650)
})

onUnmounted(() => {
  if (pollHandle) window.clearInterval(pollHandle)
})
</script>

<template>
  <div class="page-stack">
    <div v-if="loading" class="skeleton-stack">
      <div class="skeleton skeleton--hero" />
      <div class="skeleton skeleton--panel" />
    </div>

    <div v-else-if="error" class="card empty-state">
      <ShieldAlert :size="26" />
      <p>{{ error }}</p>
      <button class="btn btn-primary" @click="load(true)">Erneut versuchen</button>
    </div>

    <template v-else-if="detail">
      <div class="breadcrumb">
        <RouterLink :to="isTicket ? '/einsaetze' : '/wartungen'">
          {{ isTicket ? 'Einsätze' : 'Wartungen' }}
        </RouterLink>
        <ChevronRight :size="14" />
        <span>{{ detail.customer.short_code }} · {{ isTicket ? detail.run.period : detail.run.period }}</span>
      </div>

      <section class="card run-header">
        <div class="run-header-main">
          <p class="eyebrow">{{ detail.run.period }} · {{ detail.customer.short_code }}</p>
          <div class="title-row">
            <h2>{{ detail.run.title }}</h2>
            <StatusBadge :value="detail.run.status" kind="run" />
            <span v-if="detail.run.kind === 'ticket'" class="badge" :class="`badge--priority-${detail.run.priority}`">
              {{ priorityLabel(detail.run.priority) }}
            </span>
            <span v-else class="badge badge--kind-ticket">{{ runKindLabel(detail.run.kind) }}</span>
          </div>
          <RouterLink :to="`/kunden/${detail.customer.id}`" class="link">
            {{ detail.customer.name }}
          </RouterLink>

          <div class="run-meta-row">
            <span><CalendarDays :size="15" /> {{ formatDateTime(detail.run.scheduled_for) }}</span>
            <span><UserRound :size="15" /> {{ detail.run.assignee ?? 'Nicht zugewiesen' }}</span>
            <span><Clock3 :size="15" /> {{ formatDuration(detail.total_time_seconds) }} erfasst</span>
            <span><Cpu :size="15" /> {{ formatDuration(detail.machine_time_seconds) }} Maschinenzeit</span>
          </div>

          <p v-if="detail.run.description" class="run-description">{{ detail.run.description }}</p>

          <div v-if="detail.run.notes" class="run-note" :class="{ 'run-note--blocked': detail.run.status === 'blocked' }">
            <Info :size="16" />
            <span>{{ detail.run.notes }}</span>
          </div>
        </div>

        <div class="run-summary">
          <div
            class="progress-ring"
            :style="{ background: `conic-gradient(#14b8a6 ${progress}%, #e4e7ec 0)` }"
          >
            <div class="progress-ring-inner">
              <strong>{{ Math.round(progress) }} %</strong>
              <small>erledigt</small>
            </div>
          </div>
          <p>{{ completed }} von {{ total }} Aufgaben abgeschlossen</p>
          <ProgressBar :value="progress" />
        </div>
      </section>

      <section class="card">
        <header class="card-head">
          <div>
            <h2>Checkliste</h2>
            <p>Risikoreiche Schritte brauchen eine Freigabe. Timer läuft pro Aufgabe.</p>
          </div>
          <span class="count-pill">{{ completed }}/{{ total }}</span>
        </header>

        <div v-if="detail.tasks.length" class="task-list">
          <TaskItem
            v-for="task in detail.tasks"
            :key="task.id"
            :task="task"
            :executing="!!executing[task.id]"
            :job="jobs[task.id]"
            @execute="execute(task)"
            @approve="approve(task)"
          />
        </div>
        <div v-else class="empty-state empty-state--compact">
          <p>Dieser Wartungslauf hat noch keine Aufgaben.</p>
        </div>
      </section>

      <section class="card">
        <header class="card-head">
          <div>
            <h2>Ausführungsprotokoll</h2>
            <p>Jeder Lauf wird mit Ergebnis und Dauer dokumentiert</p>
          </div>
          <span class="count-pill">{{ detail.executions.length }}</span>
        </header>

        <div v-if="detail.executions.length" class="table-wrap">
          <table class="data-table data-table--compact">
            <thead>
              <tr>
                <th>Zeit</th>
                <th>Aufgabe</th>
                <th>Status</th>
                <th>Ergebnis</th>
                <th class="num">Dauer</th>
                <th>Version</th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="execution in detail.executions" :key="execution.id">
                <td>{{ formatDateTime(execution.created_at) }}</td>
                <td>{{ taskTitles[execution.task_id] ?? execution.task_id }}</td>
                <td>
                  <span
                    class="status-pill"
                    :class="execution.status === 'succeeded' ? 'status-pill--ok' : execution.status === 'failed' ? 'status-pill--danger' : 'status-pill--warning'"
                  >
                    <CheckCircle2 v-if="execution.status === 'succeeded'" :size="13" />
                    {{ execution.status === 'succeeded' ? 'OK' : execution.status === 'failed' ? 'Fehler' : 'Läuft' }}
                  </span>
                </td>
                <td class="cell-wrap">
                  <span v-if="execution.error" class="exec-error">{{ execution.error }}</span>
                  <template v-else>
                    <span>{{ readableResult(execution.response) }}</span>
                    <code
                      v-if="hasRawDetail(execution.response)"
                      class="exec-raw"
                      :title="execution.response ?? ''"
                    >Rohdaten</code>
                  </template>
                </td>
                <td class="num">{{ execution.duration_ms ? `${execution.duration_ms} ms` : '–' }}</td>
                <td>
                  <span v-if="execution.before_version || execution.after_version" class="version">
                    {{ execution.before_version ?? '–' }} → {{ execution.after_version ?? '–' }}
                  </span>
                  <span v-else>–</span>
                </td>
              </tr>
            </tbody>
          </table>
        </div>
        <div v-else class="empty-state empty-state--compact">
          <p>Noch keine Ausführungen protokolliert.</p>
        </div>
      </section>
    </template>
  </div>
</template>
