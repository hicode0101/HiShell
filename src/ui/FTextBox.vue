<script setup lang="ts">
import { computed, ref } from 'vue'
import { t } from '../i18n'

const props = withDefaults(
  defineProps<{
    modelValue: string | number
    placeholder?: string
    type?: 'text' | 'password'
    disabled?: boolean
    clearable?: boolean
    /** 密码字段已存储凭据：眼睛图标后显示清除图标，点击触发 clear-stored */
    storedClearable?: boolean
  }>(),
  { placeholder: '', type: 'text', disabled: false, clearable: true, storedClearable: false },
)

const emit = defineEmits<{
  (e: 'update:modelValue', v: string): void
  (e: 'clear-stored'): void
}>()

const focused = ref(false)
/** password 类型：是否以明文显示（false=圆点掩码） */
const reveal = ref(false)

const inputType = computed(() => (props.type === 'password' ? (reveal.value ? 'text' : 'password') : props.type))

function onInput(e: Event) {
  emit('update:modelValue', (e.target as HTMLInputElement).value)
}
</script>

<template>
  <div class="f-textbox" :class="{ focused, disabled, empty: !modelValue }">
    <input
      class="f-textbox-input"
      :type="inputType"
      :value="modelValue"
      :placeholder="placeholder"
      :disabled="disabled"
      @input="onInput"
      @focus="focused = true"
      @blur="focused = false"
    />
    <!-- 密码可见性切换：掩码时显示闭眼（点击查看），明文时显示睁眼（点击隐藏） -->
    <button
      v-if="type === 'password'"
      class="f-textbox-eye"
      :title="reveal ? t('ui.hidePassword') : t('ui.showPassword')"
      tabindex="-1"
      @click="reveal = !reveal"
    >
      <svg v-if="reveal" width="15" height="15" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round">
        <path d="M2.2 10s3-5.2 7.8-5.2S17.8 10 17.8 10s-3 5.2-7.8 5.2S2.2 10 2.2 10z" />
        <circle cx="10" cy="10" r="2.3" />
      </svg>
      <svg v-else width="15" height="15" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round">
        <path d="M4.3 5.6C3 6.8 2.2 8.4 2.2 10c0 0 3 5.2 7.8 5.2 1.5 0 2.8-.5 3.9-1.2M8 5.1c.6-.2 1.3-.3 2-.3 4.8 0 7.8 5.2 7.8 5.2s-.8 1.6-2.1 2.9" />
        <path d="M8.4 8.4a2.3 2.3 0 0 0 3.2 3.2" />
        <path d="M3.5 3.5l13 13" />
      </svg>
    </button>
    <!-- 已存储凭据的清除图标（扫把）：点击清除已保存的密码/口令 -->
    <button
      v-if="type === 'password' && storedClearable"
      class="f-textbox-eye"
      :title="t('ui.clearStored')"
      tabindex="-1"
      @click="emit('clearStored')"
    >
      <svg width="14" height="14" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round">
        <path d="M16.6 3.4l-5.8 5.8" />
        <path d="M10.2 9.4c-1.5-.8-3.5-.6-4.8.7l-2 2 4.5 4.5 2-2c1.3-1.3 1.5-3.3.7-4.8z" />
        <path d="M5.9 13.1l-2.4 2.4" />
      </svg>
    </button>
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
.f-textbox-eye,
.f-textbox-clear {
  border: none;
  background: transparent;
  color: var(--text-secondary);
  width: 20px;
  height: 20px;
  border-radius: 50%;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  flex: none;
  margin-left: 2px;
}
.f-textbox-eye:hover,
.f-textbox-clear:hover {
  background: var(--bg-alt-hover);
  color: var(--text-primary);
}
</style>
