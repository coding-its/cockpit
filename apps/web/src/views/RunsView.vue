<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { CalendarDays, RefreshCw, ShieldAlert, Wrench } from 'lucide-vue-next'
import { api, errorMessage } from '../api'
import type { RunListItem, RunStatus } from '../types'
import { runStatusLabel } from '../format'
import RunCard from '../components/RunCard.vue'

const runs = ref<RunListItem[]>([])
const activeFilter = ref<'all' | RunStatus>('all')
const loading = ref(true)
const error = ref('')

const filters: Array<{ value: 'all' | RunStatus; label: string }> = [
  { value: 'all', label: 'Alle' },
  { value: 'planned', label: 'Geplant' },
  { value: 'in_progress', label: 'Läuft' },
  { value: 'blocked', label: 'Blockiert' },
  { value: 'done', label: 'Abgeschlossen' },
]

const filtered = computed(() => {
  if (activeFilter.value === 'all') return runs.value
  return runs.value.filter((run) => run.status === activeFilter.value)
})

function countFor(status: 'all' | RunStatus) {
  if (status === 'all') return runs.value.length
  return runs.value.filter((run) => run.status === status).length
}

async function load() {
  loading.value = true
  error.value = ''
  try {
    runs.value = await api.runs()
  } catch (caught) {
    error.value = errorMessage(caught)
  } finally {
    loading.value = false
  }
}

onMounted(load)
</script>

<template>
  <div class="page-stack">
    <div class="page-toolbar page-toolbar--wrap">
      <div class="filter-chips">
        <button
          v-for="filter in filters"
          :key="filter.value"
          class="filter-chip"
          :class="{ 'is-active': activeFilter === filter.value }"
          @click="activeFilter = filter.value"
        >
          {{ filter.label }}
          <span>{{ countFor(filter.value) }}</span>
        </button>
      </div>
      <button class="btn btn-ghost" :disabled="loading" @click="load">
        <RefreshCw :size="15" :class="{ spin: loading }" />
        Aktualisieren
      </button>
    </div>

    <div v-if="loading" class="run-grid">
      <div v-for="index in 4" :key="index" class="skeleton skeleton--run" />
    </div>

    <div v-else-if="error" class="card empty-state">
      <ShieldAlert :size="26" />
      <p>{{ error }}</p>
      <button class="btn btn-primary" @click="load">Erneut versuchen</button>
    </div>

    <template v-else>
      <div v-if="filtered.length" class="run-grid run-grid--wide">
        <RunCard v-for="run in filtered" :key="run.id" :run="run" />
      </div>

      <div v-else class="card empty-state">
        <Wrench :size="28" />
        <h2>Keine Wartungen in dieser Ansicht</h2>
        <p>
          {{ activeFilter === 'all' ? 'Es sind noch keine Wartungsläufe angelegt.' : `Es gibt aktuell keine Läufe mit Status „${runStatusLabel(activeFilter)}“.` }}
        </p>
      </div>

      <section class="info-strip">
        <div class="info-strip-icon"><CalendarDays :size="18" /></div>
        <p>
          <strong>Wartungsläufe entstehen aus Verträgen.</strong>
          Jeder Lauf hat eine Checkliste aus NinjaOne- und WordPress-Aufgaben. Risikoreiche Schritte
          werden erst nach Freigabe ausgeführt.
        </p>
      </section>
    </template>
  </div>
</template>
