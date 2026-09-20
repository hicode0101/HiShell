<script setup lang="ts">
import { watch } from 'vue'

const props = withDefaults(
  defineProps<{
    open: boolean
    title: string
    width?: number
  }>(),
  { width: 560 },
)

const emit = defineEmits<{ (e: 'update:open', v: boolean): void }>()

function close() {
  emit('update:open', false)
}

function onKeydown(e: KeyboardEvent) {
  if (e.key === 'Escape' && props.open) close()
}
watch(
  () => props.open,
  (v) => {
    if (typeof document !== 'undefined') {
      if (v) document.addEventListener('keydown', onKeydown)
      else document.removeEventListener('keydown', onKeydown)
    }
  },
)
</script>

<template>
  <Teleport to="body">
    <div v-if="open" class="f-dialog-overlay">
      <div class="f-dialog" :style="{ width: width + 'px' }" role="dialog" aria-modal="true">
        <div class="f-dialog-title">{{ title }}</div>
        <div class="f-dialog-body">
          <slot />
        </div>
        <div v-if="$slots.footer" class="f-dialog-footer">
          <slot name="footer" />
        </div>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
.f-dialog-overlay {
  position: fixed;
  inset: 0;
  z-index: 2000;
  background: var(--bg-smoke);
  display: flex;
  align-items: center;
  justify-content: center;
  animation: hishell-fade-in 0.14s ease;
}
.f-dialog {
  max-width: calc(100vw - 48px);
  max-height: calc(100vh - 48px);
  display: flex;
  flex-direction: column;
  background: var(--bg-layer);
  border: 1px solid var(--stroke-card);
  border-radius: var(--radius-lg);
  box-shadow: var(--shadow-dialog);
  overflow: hidden;
}
.f-dialog-title {
  font-size: 18px;
  font-weight: 600;
  padding: 18px 24px 8px;
  color: var(--text-primary);
}
.f-dialog-body {
  padding: 8px 24px 16px;
  overflow: auto;
  flex: 1;
}
.f-dialog-footer {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  padding: 16px 24px;
  background: var(--bg-card);
  border-top: 1px solid var(--stroke-card);
}
</style>
