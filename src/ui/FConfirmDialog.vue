<script setup lang="ts">
/**
 * 通用确认对话框（确定 / 取消）：替代"同按钮点两次"的二次确认方式，
 * 用于删除等不可恢复操作的确认。
 */
import { computed } from 'vue'
import FButton from './FButton.vue'
import FDialog from './FDialog.vue'
import { t } from '../i18n'

const props = withDefaults(
  defineProps<{
    open: boolean
    title: string
    message: string
    confirmText?: string
    cancelText?: string
    danger?: boolean
  }>(),
  { confirmText: '', cancelText: '', danger: true },
)

const emit = defineEmits<{
  (e: 'update:open', v: boolean): void
  (e: 'confirm'): void
}>()

const confirmLabel = computed(() => props.confirmText || t('common.confirm'))
const cancelLabel = computed(() => props.cancelText || t('common.cancel'))

function confirm() {
  emit('confirm')
  emit('update:open', false)
}
</script>

<template>
  <FDialog
    :open="open"
    :title="title"
    :width="420"
    enter-confirm
    @update:open="emit('update:open', $event)"
    @confirm="confirm"
  >
    <div class="f-confirm-message">{{ message }}</div>
    <template #footer>
      <FButton @click="emit('update:open', false)">{{ cancelLabel }}</FButton>
      <FButton :variant="danger ? 'danger' : 'accent'" @click="confirm">{{ confirmLabel }}</FButton>
    </template>
  </FDialog>
</template>

<style scoped>
.f-confirm-message {
  font-size: 13px;
  color: var(--text-primary);
  line-height: 1.7;
  padding: 6px 2px 10px;
  word-break: break-all;
}
</style>
