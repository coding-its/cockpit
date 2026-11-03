<script setup lang="ts">
import { AlertCircle, CheckCircle2, Info, X } from 'lucide-vue-next'
import { useAppStore } from '../stores/app'

const store = useAppStore()
</script>

<template>
  <div class="toast-stack" aria-live="polite">
    <TransitionGroup name="toast">
      <div v-for="toast in store.toasts" :key="toast.id" class="toast" :class="`toast--${toast.kind}`">
        <CheckCircle2 v-if="toast.kind === 'success'" :size="18" />
        <AlertCircle v-else-if="toast.kind === 'error'" :size="18" />
        <Info v-else :size="18" />
        <span>{{ toast.message }}</span>
        <button class="toast-close" aria-label="Hinweis schließen" @click="store.dismiss(toast.id)">
          <X :size="14" />
        </button>
      </div>
    </TransitionGroup>
  </div>
</template>
