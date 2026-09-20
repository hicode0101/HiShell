<script setup lang="ts">
defineProps<{
  modelValue: string
  options: { label: string; value: string; disabled?: boolean }[]
  disabled?: boolean
}>()
const emit = defineEmits<{ (e: 'update:modelValue', v: string): void }>()
</script>

<template>
  <div class="f-radio-group">
    <label
      v-for="opt in options"
      :key="opt.value"
      class="f-radio"
      :class="{ disabled: opt.disabled || disabled, checked: modelValue === opt.value }"
      @click="!(opt.disabled || disabled) && emit('update:modelValue', opt.value)"
    >
      <span class="f-radio-dot">
        <span v-if="modelValue === opt.value" class="inner" />
      </span>
      <span class="f-radio-label">{{ opt.label }}</span>
    </label>
  </div>
</template>

<style scoped>
.f-radio-group {
  display: flex;
  flex-wrap: wrap;
  gap: 4px 18px;
}
.f-radio {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  cursor: pointer;
}
.f-radio.disabled {
  cursor: default;
  color: var(--text-disabled);
}
.f-radio-dot {
  width: 18px;
  height: 18px;
  border-radius: 50%;
  border: 1px solid var(--stroke-strong);
  background: var(--bg-card);
  display: inline-flex;
  align-items: center;
  justify-content: center;
  flex: none;
}
.f-radio.checked .f-radio-dot {
  border-color: var(--accent);
}
.f-radio-dot .inner {
  width: 10px;
  height: 10px;
  border-radius: 50%;
  background: var(--accent);
}
.f-radio.disabled .f-radio-dot .inner {
  background: var(--text-disabled);
}
.f-radio-label {
  font-size: 13px;
}
</style>
