<script setup lang="ts">
import { ref } from 'vue'
import { t } from '../i18n'

const props = withDefaults(
  defineProps<{
    modelValue: string | number
    placeholder?: string
    type?: 'text' | 'password'
    disabled?: boolean
    clearable?: boolean
  }>(),
  { placeholder: '', type: 'text', disabled: false, clearable: true },
)

const emit = defineEmits<{ (e: 'update:modelValue', v: string): void }>()

const focused = ref(false)

function onInput(e: Event) {
  emit('update:modelValue', (e.target as HTMLInputElement).value)
}
</script>

<template>
  <div class="f-textbox" :class="{ focused, disabled, empty: !modelValue }">
    <input
      class="f-textbox-input"
      :type="type"
      :value="modelValue"
      :placeholder="placeholder"
      :disabled="disabled"
      @input="onInput"
      @focus="focused = true"
      @blur="focused = false"
    />
    <button
      v-if="clearable && modelValue && !disabled"
      class="f-textbox-clear"
      :title="t('ui.clear')"
      tabindex="-1"
      @click="emit('update:modelValue', '')"
    >
      <svg width="12" height="12" viewBox="0 0 12 12" stroke="currentColor" stroke-width="1.2" stroke-linecap="round">
        <path d="M2.5 2.5l7 7M9.5 2.5l-7 7" />
      </svg>
    </button>
  </div>
</template>

<style scoped>
.f-textbox {
  display: inline-flex;
  align-items: center;
  height: 30px;
  min-width: 0;
  border-radius: var(--radius-sm);
  border: 1px solid var(--stroke-control);
  border-bottom-width: 2px; /* Fluent 下划线式焦点 */
  background: var(--bg-card);
  padding: 0 6px 0 10px;
  width: 100%;
}
.f-textbox.focused {
  border-bottom-color: var(--accent);
}
.f-textbox.disabled {
  background: var(--bg-alt-pressed);
  color: var(--text-disabled);
}
.f-textbox-input {
  flex: 1;
  min-width: 0;
  border: none;
  outline: none;
  background: transparent;
  font: inherit;
  font-size: 13px;
  color: var(--text-primary);
  padding: 0;
}
.f-textbox-input::placeholder {
  color: var(--text-tertiary);
}
.f-textbox-clear {
  border: none;
  background: transparent;
  color: var(--text-secondary);
  width: 18px;
  height: 18px;
  border-radius: 50%;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  flex: none;
}
.f-textbox-clear:hover {
  background: var(--bg-alt-hover);
}
</style>
