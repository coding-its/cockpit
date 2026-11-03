<script setup lang="ts">
withDefaults(
  defineProps<{
    value: number
    tone?: 'teal' | 'amber' | 'red'
  }>(),
  { tone: 'teal' },
)
</script>

<template>
  <div class="progress">
    <div
      class="progress-fill"
      :class="`tone-${tone}`"
      :style="{ width: `${Math.min(100, Math.max(0, value))}%` }"
    />
  </div>
</template>
