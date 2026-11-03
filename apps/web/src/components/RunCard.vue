<script setup lang="ts">
import { computed } from 'vue'
import { ArrowUpRight, CalendarDays } from 'lucide-vue-next'
import type { RunListItem } from '../types'
import { formatDateTime } from '../format'
import ProgressBar from './ProgressBar.vue'
import StatusBadge from './StatusBadge.vue'

const props = defineProps<{ run: RunListItem }>()

const progress = computed(() => {
  if (!props.run.total_tasks) return 0
  return (props.run.done_tasks / props.run.total_tasks) * 100
})

const tone = computed(() => {
  if (props.run.status === 'blocked') return 'red'
  if (props.run.status === 'done') return 'teal'
  return 'amber'
})
</script>

<template>
  <RouterLink :to="`/wartungen/${run.id}`" class="run-card">
    <div class="run-card-top">
      <StatusBadge :value="run.status" kind="run" />
      <span class="run-date">
        <CalendarDays :size="14" />
        {{ formatDateTime(run.scheduled_for) }}
      </span>
    </div>

    <div class="run-card-body">
      <p class="run-customer">{{ run.customer_name }}</p>
      <h3>{{ run.title }}</h3>
    </div>

    <div class="run-card-progress">
      <div class="run-progress-label">
        <span>{{ run.done_tasks }} von {{ run.total_tasks }} Aufgaben</span>
        <strong>{{ Math.round(progress) }} %</strong>
      </div>
      <ProgressBar :value="progress" :tone="tone" />
    </div>

    <div class="run-card-foot">
      <span>{{ run.assignee ?? 'Nicht zugewiesen' }}</span>
      <span class="run-open">
        Öffnen
        <ArrowUpRight :size="14" />
      </span>
    </div>
  </RouterLink>
</template>
