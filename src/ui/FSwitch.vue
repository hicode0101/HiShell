<script setup lang="ts">
defineProps<{ modelValue: boolean; disabled?: boolean }>()
const emit = defineEmits<{ (e: 'update:modelValue', v: boolean): void }>()
</script>

<template>
  <button
    class="f-switch"
    :class="{ on: modelValue, disabled }"
    type="button"
    role="switch"
    :aria-checked="modelValue"
    :disabled="disabled"
    @click="emit('update:modelValue', !modelValue)"
  >
    <span class="thumb" />
  </button>
</template>

<style scoped>
.f-switch {
  width: 40px;
  height: 20px;
  border-radius: 10px;
  border: 1px solid var(--stroke-strong);
  background: var(--bg-card);
  position: relative;
  cursor: pointer;
  padding: 0;
  transition: background 0.12s ease, border-color 0.12s ease;
}
.f-switch .thumb {
  position: absolute;
  top: 3px;
  left: 3px;
  width: 12px;
  height: 12px;
  border-radius: 50%;
  background: var(--text-secondary);
  transition: left 0.14s ease, background 0.12s ease;
}
.f-switch.on {
  background: var(--accent);
  border-color: var(--accent);
}
.f-switch.on .thumb {
  left: 23px;
  background: var(--accent-text-on);
}
.f-switch.disabled {
  opacity: 0.45;
  cursor: default;
}
</style>
