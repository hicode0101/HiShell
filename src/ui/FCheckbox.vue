<script setup lang="ts">
defineProps<{ modelValue: boolean; label?: string; disabled?: boolean }>()
const emit = defineEmits<{ (e: 'update:modelValue', v: boolean): void }>()
</script>

<template>
  <label class="f-checkbox" :class="{ disabled }">
    <span class="f-checkbox-box" :class="{ checked: modelValue }" @click="!disabled && emit('update:modelValue', !modelValue)">
      <svg v-if="modelValue" width="12" height="12" viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
        <path d="M2.5 6.5l2.5 2.5 4.5-5.5" />
      </svg>
    </span>
    <span v-if="label" class="f-checkbox-label">{{ label }}</span>
  </label>
</template>

<style scoped>
.f-checkbox {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  cursor: pointer;
  color: var(--text-primary);
}
.f-checkbox.disabled {
  color: var(--text-disabled);
  cursor: default;
}
.f-checkbox-box {
  width: 18px;
  height: 18px;
  border-radius: var(--radius-sm);
  border: 1px solid var(--stroke-strong);
  background: var(--bg-card);
  display: inline-flex;
  align-items: center;
  justify-content: center;
  color: var(--accent-text-on);
  flex: none;
  transition: background 0.1s ease;
}
.f-checkbox-box:hover {
  background: var(--bg-alt-hover);
}
.f-checkbox-box.checked {
  background: var(--accent);
  border-color: var(--accent);
}
.f-checkbox.disabled .f-checkbox-box.checked {
  background: var(--text-disabled);
  border-color: transparent;
}
.f-checkbox-label {
  font-size: 13px;
}
</style>
