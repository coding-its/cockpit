<script setup lang="ts">
import { computed } from 'vue'
import {
  riskLabel,
  runStatusLabel,
  siteStatusLabel,
  sourceLabel,
  taskStatusLabel,
} from '../format'

const props = defineProps<{
  value: string
  kind?: 'run' | 'task' | 'risk' | 'site' | 'source'
}>()

const label = computed(() => {
  switch (props.kind) {
    case 'run':
      return runStatusLabel(props.value)
    case 'task':
      return taskStatusLabel(props.value)
    case 'risk':
      return riskLabel(props.value)
    case 'site':
      return siteStatusLabel(props.value)
    case 'source':
      return sourceLabel(props.value)
    default:
      return props.value
  }
})
</script>

<template>
  <span class="badge" :class="[`badge--${kind ?? 'plain'}`, `badge--${value}`]">
    <span class="badge-dot" />
    {{ label }}
  </span>
</template>
