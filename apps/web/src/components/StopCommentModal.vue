<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { MessageSquarePlus, SkipForward } from 'lucide-vue-next'
import { api, errorMessage } from '../api'
import { formatDuration } from '../format'
import { useAppStore } from '../stores/app'
import Modal from './Modal.vue'

/**
 * Fragt nach dem Kommentar, sobald ein Timer gestoppt wurde.
 *
 * Hängt an `store.pendingComment`, das jeder Stopp füllt – egal ob über die
 * Kopfleiste, das Geräte-Panel oder den Web-Kunden. Der Kommentar landet direkt
 * am eben beendeten Zeiteintrag, damit die Abrechnung nicht später nachgeholt
 * werden muss.
 */
const store = useAppStore()

const text = ref('')
const saving = ref(false)

const entry = computed(() => store.pendingComment)
const open = computed(() => !!entry.value)

watch(entry, (value) => {
  if (value) text.value = ''
})

async function save() {
  const current = entry.value
  if (!current) return

  const comment = text.value.trim()
  if (!comment) {
    store.pendingComment = null
    return
  }

  saving.value = true
  try {
    await api.setTimeComment(current.id, comment)
    store.pendingComment = null
    store.toast('Kommentar gespeichert.', 'success')
  } catch (caught) {
    store.toast(errorMessage(caught), 'error')
  } finally {
    saving.value = false
  }
}

function skip() {
  store.pendingComment = null
}
</script>

<template>
  <Modal
    :open="open"
    title="Was wurde gemacht?"
    :subtitle="
      entry
        ? `Zeit gestoppt: ${entry.activity} · ${formatDuration(entry.duration_seconds)} · ${entry.customer_name}`
        : ''
    "
    @close="skip"
  >
    <label class="field">
      <span>Kommentar für die Abrechnung</span>
      <textarea
        v-model="text"
        rows="4"
        autofocus
        placeholder="z. B. Patches installiert, zwei Geräte neu gestartet, Kunde informiert"
        @keydown.ctrl.enter="save"
        @keydown.meta.enter="save"
      />
    </label>

    <p class="form-hint">
      Der Kommentar steht später im Monatsbericht beim Kunden. Mit
      <kbd>Strg</kbd> + <kbd>Enter</kbd> speichern.
    </p>

    <template #footer>
      <button class="btn btn-ghost" type="button" @click="skip">
        <SkipForward :size="15" />
        Ohne Kommentar
      </button>
      <button class="btn btn-primary" type="button" :disabled="saving" @click="save">
        <MessageSquarePlus :size="15" />
        {{ saving ? 'Speichert …' : 'Kommentar speichern' }}
      </button>
    </template>
  </Modal>
</template>
