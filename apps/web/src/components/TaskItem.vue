<script setup lang="ts">
import { computed } from 'vue'
import {
  CheckCircle2,
  HeartPulse,
  Lock,
  Monitor,
  PenLine,
  Play,
  Puzzle,
  RefreshCw,
  ShieldCheck,
  Terminal,
  Wrench,
} from 'lucide-vue-next'
import type { Job, Task } from '../types'
import { taskKindLabel } from '../format'
import ProgressBar from './ProgressBar.vue'
import StatusBadge from './StatusBadge.vue'

const props = defineProps<{
  task: Task
  executing: boolean
  job?: Job | null
}>()

const emit = defineEmits<{
  execute: []
  approve: []
}>()

const kindIcon = computed(() => {
  switch (props.task.kind) {
    case 'NINJA_PATCH':
      return Wrench
    case 'NINJA_SCRIPT':
      return Terminal
    case 'WP_PLUGIN_UPDATE':
      return Puzzle
    case 'WP_CORE_UPDATE':
      return RefreshCw
    case 'WP_HEALTH':
      return HeartPulse
    default:
      return PenLine
  }
})

const needsApproval = computed(() => props.task.requires_approval && !props.task.approved)
</script>

<template>
  <article class="task" :class="[`task--${task.status}`, { 'task--locked': needsApproval }]">
    <div class="task-kind" :class="`task-kind--${task.kind.toLowerCase()}`">
      <component :is="kindIcon" :size="18" />
    </div>

    <div class="task-main">
      <div class="task-head">
        <div class="task-title">
          <span class="task-position">{{ task.position }}</span>
          <div>
            <h3>{{ task.title }}</h3>
            <p v-if="task.description">{{ task.description }}</p>
          </div>
        </div>
        <div class="task-badges">
          <StatusBadge v-if="task.risk !== 'low'" :value="task.risk" kind="risk" />
          <StatusBadge :value="task.status" kind="task" />
        </div>
      </div>

      <div class="task-meta">
        <span class="meta-item">
          <Monitor :size="14" />
          {{ task.target_label ?? 'Ohne Ziel' }}
        </span>
        <span class="meta-item meta-item--muted">{{ taskKindLabel(task.kind) }}</span>
        <span v-if="needsApproval" class="meta-item meta-item--warning">
          <Lock :size="14" />
          Freigabe erforderlich
        </span>
      </div>

      <div v-if="task.result_summary" class="task-result">
        <CheckCircle2 :size="16" />
        <span>{{ task.result_summary }}</span>
      </div>

      <div v-if="job && (job.status === 'queued' || job.status === 'running')" class="job-progress">
        <div class="job-line">
          <span>{{ job.message ?? 'Job läuft' }}</span>
          <strong>{{ job.progress }} %</strong>
        </div>
        <ProgressBar :value="job.progress" tone="teal" />
      </div>
    </div>

    <div class="task-actions">
      <button
        v-if="needsApproval"
        class="btn btn-amber btn-sm"
        @click="emit('approve')"
      >
        <ShieldCheck :size="15" />
        Freigeben
      </button>
      <button
        v-else-if="task.status !== 'done'"
        class="btn btn-primary btn-sm"
        :disabled="executing"
        @click="emit('execute')"
      >
        <Play :size="15" />
        {{ executing ? 'Läuft …' : task.kind === 'MANUAL' ? 'Bestätigen' : 'Ausführen' }}
      </button>

    </div>
  </article>
</template>
