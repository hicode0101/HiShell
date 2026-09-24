<script setup lang="ts">
import { ref, watch } from 'vue'

const props = withDefaults(
  defineProps<{
    open: boolean
    title: string
    width?: number
    /** 打开时按回车触发 confirm（用于删除确认等无输入框的对话框） */
    enterConfirm?: boolean
  }>(),
  { width: 560, enterConfirm: false },
)

const emit = defineEmits<{
  (e: 'update:open', v: boolean): void
  (e: 'confirm'): void
}>()

const overlayEl = ref<HTMLElement>()

function close() {
  emit('update:open', false)
}

function onKeydown(e: KeyboardEvent) {
  // 多层对话框叠开时，Esc 只关闭最上层（DOM 中最后一个打开的遮罩）
  if (e.key === 'Escape' && props.open) {
    const overlays = document.querySelectorAll('.f-dialog-overlay')
    if (!overlays.length || overlays[overlays.length - 1] === overlayEl.value) close()
  }
  // 回车确认：只在最上层对话框生效；焦点在按钮/输入控件上时交给原生行为，避免重复触发
  if (e.key === 'Enter' && props.enterConfirm && props.open && !e.isComposing) {
    const el = e.target as HTMLElement | null
    if (el && el.closest('button, input, textarea, select, [contenteditable]')) return
    const overlays = document.querySelectorAll('.f-dialog-overlay')
    if (!overlays.length || overlays[overlays.length - 1] === overlayEl.value) {
      e.preventDefault()
      emit('confirm')
    }
  }
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
    <div v-if="open" ref="overlayEl" class="f-dialog-overlay">
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
