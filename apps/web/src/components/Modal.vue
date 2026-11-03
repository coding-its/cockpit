<script setup lang="ts">
import { onUnmounted, ref, watch } from 'vue'
import { X } from 'lucide-vue-next'

const props = defineProps<{
  open: boolean
  title: string
  subtitle?: string
}>()

const emit = defineEmits<{ close: [] }>()

/** Verschiebung gegenüber der Bildschirmmitte – der Dialog darf Platz machen. */
const offset = ref({ x: 0, y: 0 })
const dragging = ref(false)
const modalEl = ref<HTMLElement | null>(null)
let origin = { x: 0, y: 0, offsetX: 0, offsetY: 0 }

/** Wie viel vom Dialog mindestens im Fenster bleiben muss. */
const KEEP_VISIBLE = 180

/**
 * Grenzen für die Verschiebung, damit der Dialog nicht aus dem Fenster
 * geschoben wird und der Kopf zum Zurückziehen erreichbar bleibt.
 */
function clampToWindow(next: { x: number; y: number }) {
  const element = modalEl.value
  if (!element) return next

  const rect = element.getBoundingClientRect()
  const base = {
    left: rect.left - offset.value.x,
    top: rect.top - offset.value.y,
    width: rect.width,
    height: rect.height,
  }

  const vw = window.innerWidth
  const vh = window.innerHeight
  const minX = KEEP_VISIBLE - (base.left + base.width)
  const maxX = vw - KEEP_VISIBLE - base.left
  const minY = 8 - base.top
  const maxY = vh - 140 - base.top

  return {
    x: Math.min(Math.max(next.x, minX), Math.max(minX, maxX)),
    y: Math.min(Math.max(next.y, minY), Math.max(minY, maxY)),
  }
}

function onKeydown(event: KeyboardEvent) {
  if (event.key === 'Escape' && props.open) {
    emit('close')
  }
}

function startDrag(event: PointerEvent) {
  if (event.button !== 0) return
  // Knöpfe im Kopf sollen klickbar bleiben, nicht den Dialog ziehen.
  if ((event.target as HTMLElement).closest('button')) return

  dragging.value = true
  origin = {
    x: event.clientX,
    y: event.clientY,
    offsetX: offset.value.x,
    offsetY: offset.value.y,
  }
  window.addEventListener('pointermove', moveDrag)
  window.addEventListener('pointerup', stopDrag)
  window.addEventListener('pointercancel', stopDrag)
}

function moveDrag(event: PointerEvent) {
  if (!dragging.value) return
  offset.value = clampToWindow({
    x: origin.offsetX + event.clientX - origin.x,
    y: origin.offsetY + event.clientY - origin.y,
  })
}

function stopDrag() {
  dragging.value = false
  window.removeEventListener('pointermove', moveDrag)
  window.removeEventListener('pointerup', stopDrag)
  window.removeEventListener('pointercancel', stopDrag)
}

// Jeder neue Dialog startet mittig.
watch(
  () => props.open,
  (open) => {
    if (open) offset.value = { x: 0, y: 0 }
    else stopDrag()
  },
)

window.addEventListener('keydown', onKeydown)
onUnmounted(() => {
  window.removeEventListener('keydown', onKeydown)
  stopDrag()
})
</script>

<template>
  <!--
    Bewusst ohne <Transition>: Ein hängender Übergang kann das Fenster im DOM
    stehen lassen, obwohl es geschlossen ist – dann blockiert der Hintergrund
    die ganze Oberfläche. Die Einblendung übernimmt eine CSS-Animation, das
    Ausblenden passiert sofort mit dem v-if.
  -->
  <Teleport to="body">
      <div v-if="open" class="modal-backdrop" @click.self="emit('close')">
        <div
          ref="modalEl"
          class="modal"
          :class="{ 'is-dragging': dragging }"
          role="dialog"
          aria-modal="true"
          :aria-label="title"
          :style="{ '--modal-x': `${offset.x}px`, '--modal-y': `${offset.y}px` }"
        >
          <header class="modal-head" title="Zum Verschieben ziehen" @pointerdown="startDrag">
            <div>
              <h2>{{ title }}</h2>
              <p v-if="subtitle">{{ subtitle }}</p>
            </div>
            <button class="icon-btn" aria-label="Dialog schließen" @click="emit('close')">
              <X :size="18" />
            </button>
          </header>
          <div class="modal-body">
            <slot />
          </div>
          <footer v-if="$slots.footer" class="modal-foot">
            <slot name="footer" />
          </footer>
        </div>
      </div>
  </Teleport>
</template>
