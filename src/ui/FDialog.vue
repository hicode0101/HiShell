<script setup lang="ts">
import { nextTick, ref, watch } from 'vue'

const props = withDefaults(
  defineProps<{
    open: boolean
    title: string
    width?: number
    /** 打开时按回车触发 confirm（用于删除确认等无输入框的对话框） */
    enterConfirm?: boolean
    /** 打开时聚焦首个文本输入框并全选（重命名/新建类：键入直接替换预填内容） */
    autofocusSelect?: boolean
  }>(),
  { width: 560, enterConfirm: false, autofocusSelect: false },
)

const emit = defineEmits<{
  (e: 'update:open', v: boolean): void
  (e: 'confirm'): void
}>()

const overlayEl = ref<HTMLElement>()
const panelEl = ref<HTMLElement>()

function close() {
  emit('update:open', false)
}

function onKeydown(e: KeyboardEvent) {
  // 多层对话框叠开时，Esc 只关闭最上层（DOM 中最后一个打开的遮罩）
  if (e.key === 'Escape' && props.open) {
    const overlays = document.querySelectorAll('.f-dialog-overlay')
    if (!overlays.length || overlays[overlays.length - 1] === overlayEl.value) close()
  }
  // 回车确认：只在最上层对话框生效。焦点在本对话框内的按钮/输入控件上时交给原生行为（按钮回车即点击），
  // 避免重复触发；焦点在对话框外（如残留的触发按钮）时不拦截，仍执行确认
  if (e.key === 'Enter' && props.enterConfirm && props.open && !e.isComposing) {
    const el = e.target as HTMLElement | null
    if (el && overlayEl.value?.contains(el) && el.closest('button, input, textarea, select, [contenteditable]')) return
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
    if (typeof document === 'undefined') return
    if (v) {
      document.addEventListener('keydown', onKeydown)
      // 打开时把焦点移入对话框：优先首个可见、可编辑的文本输入框（键盘直接键入，无需再点一下），
      // 没有合适输入框时聚焦面板本身：否则焦点残留在打开对话框的按钮上，回车会再次激活该按钮而非确认对话框
      nextTick(() => {
        const panel = panelEl.value
        if (!panel) return
        const selector =
          'input:not([type="checkbox"]):not([type="radio"]):not([type="hidden"]):not([type="button"]):not([type="submit"]):not([type="reset"]):not([type="file"]), textarea'
        for (const el of panel.querySelectorAll<HTMLInputElement | HTMLTextAreaElement>(selector)) {
          if (el.disabled || el.readOnly || !el.offsetParent) continue
          el.focus()
          try {
            if (props.autofocusSelect) el.select()
            else el.setSelectionRange(el.value.length, el.value.length)
          } catch {
            /* number/email 等类型不支持选区 API，忽略 */
          }
          return
        }
        panel.focus()
      })
    } else {
      document.removeEventListener('keydown', onKeydown)
    }
  },
)
</script>

<template>
  <Teleport to="body">
    <div v-if="open" ref="overlayEl" class="f-dialog-overlay">
      <div ref="panelEl" class="f-dialog" :style="{ width: width + 'px' }" role="dialog" aria-modal="true" tabindex="-1">
        <div class="f-dialog-title">
          <span>{{ title }}</span>
          <slot name="title-extra" />
        </div>
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
.f-dialog:focus,
.f-dialog:focus-visible {
  outline: none;
}
.f-dialog-title {
  display: flex;
  align-items: baseline;
  gap: 10px;
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
