<script setup lang="ts">
import { ref } from 'vue'
import FIcon from './FIcon.vue'

const props = defineProps<{
  modelValue: string
  options: string[]
  placeholder?: string
  disabled?: boolean
}>()

const emit = defineEmits<{ (e: 'update:modelValue', v: string): void }>()

const open = ref(false)
const rootEl = ref<HTMLElement>()

function pick(v: string) {
  emit('update:modelValue', v)
  open.value = false
}

function onDocClick(e: MouseEvent) {
  if (rootEl.value && !rootEl.value.contains(e.target as Node)) open.value = false
}
if (typeof document !== 'undefined') {
  document.addEventListener('click', onDocClick, true)
}
</script>

<template>
  <div ref="rootEl" class="f-combo" :class="{ disabled }">
    <button class="f-combo-face" :disabled="disabled" type="button" @click="open = !open">
      <span class="f-combo-value" :class="{ placeholder: !modelValue }">{{ modelValue || placeholder || '请选择' }}</span>
      <FIcon name="chevron-down" :size="14" />
    </button>
    <div v-if="open" class="f-combo-popup">
      <div
        v-for="opt in options"
        :key="opt"
        class="f-combo-item"
        :class="{ selected: opt === modelValue }"
        @click="pick(opt)"
      >
        <span>{{ opt }}</span>
        <FIcon v-if="opt === modelValue" name="check" :size="14" />
      </div>
    </div>
  </div>
</template>

<style scoped>
.f-combo {
  position: relative;
  width: 100%;
}
.f-combo-face {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 6px;
  width: 100%;
  height: 30px;
  padding: 0 8px 0 10px;
  border-radius: var(--radius-sm);
  border: 1px solid var(--stroke-control);
  border-bottom-width: 2px;
  background: var(--bg-card);
  color: var(--text-primary);
  font: inherit;
  font-size: 13px;
  cursor: pointer;
}
.f-combo-face:hover:not(:disabled) {
  background: var(--bg-alt-hover);
}
.f-combo.disabled .f-combo-face {
  background: var(--bg-alt-pressed);
  color: var(--text-disabled);
  cursor: default;
}
.f-combo-value.placeholder {
  color: var(--text-tertiary);
}
.f-combo-popup {
  position: absolute;
  top: calc(100% + 4px);
  left: 0;
  right: 0;
  z-index: 1000;
  max-height: 220px;
  overflow: auto;
  background: var(--bg-card);
  border: 1px solid var(--stroke-card);
  border-radius: var(--radius-md);
  box-shadow: var(--shadow-flyout);
  padding: 4px;
  animation: hishell-fade-in 0.12s ease;
}
.f-combo-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  padding: 6px 10px;
  border-radius: var(--radius-sm);
  cursor: pointer;
  color: var(--text-primary);
}
.f-combo-item:hover {
  background: var(--bg-alt-hover);
}
.f-combo-item.selected {
  font-weight: 600;
  color: var(--accent);
}
</style>
