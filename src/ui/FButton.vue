<script setup lang="ts">
import FIcon from './FIcon.vue'

withDefaults(
  defineProps<{
    variant?: 'accent' | 'standard' | 'subtle' | 'danger'
    icon?: string
    disabled?: boolean
    title?: string
  }>(),
  { variant: 'standard', icon: '', disabled: false, title: '' },
)
</script>

<template>
  <button class="f-btn" :class="[variant]" :disabled="disabled" :title="title" type="button">
    <FIcon v-if="icon" :name="icon" :size="18" />
    <span v-if="$slots.default" class="f-btn-label"><slot /></span>
  </button>
</template>

<style scoped>
.f-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  height: 30px;
  padding: 0 12px;
  border-radius: var(--radius-sm);
  border: 1px solid transparent;
  font: inherit;
  font-size: 13px;
  color: var(--text-primary);
  background: var(--bg-card);
  border-color: var(--stroke-control);
  cursor: pointer;
  transition: background 0.1s ease;
  white-space: nowrap;
}
.f-btn:hover:not(:disabled) {
  background: var(--bg-alt-hover);
}
.f-btn:active:not(:disabled) {
  background: var(--bg-alt-pressed);
  color: var(--text-secondary);
}
.f-btn:disabled {
  color: var(--text-disabled);
  background: var(--bg-alt-pressed);
  border-color: var(--stroke-card);
  cursor: default;
}

.f-btn.accent {
  background: var(--accent);
  color: var(--accent-text-on);
  border-color: transparent;
  font-weight: 600;
}
.f-btn.accent:hover:not(:disabled) {
  background: var(--accent-hover);
}
.f-btn.accent:active:not(:disabled) {
  background: var(--accent-pressed);
}

.f-btn.subtle {
  background: transparent;
  border-color: transparent;
}

.f-btn.danger {
  color: var(--danger);
}
.f-btn.danger:hover:not(:disabled) {
  background: color-mix(in srgb, var(--danger) 10%, transparent);
}

.f-btn.icon-only {
  padding: 0;
  width: 30px;
}

.f-btn-label {
  line-height: 1;
  transform: translateY(0.5px);
}
</style>
