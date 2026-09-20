<script setup lang="ts">
import { toastState } from './toast'
import FIcon from './FIcon.vue'
</script>

<template>
  <div class="toast-host">
    <TransitionGroup name="toast">
      <div v-for="t in toastState.items" :key="t.id" class="toast" :class="t.kind">
        <FIcon :name="t.kind === 'error' ? 'info' : t.kind === 'success' ? 'check' : 'info'" :size="16" />
        <span>{{ t.text }}</span>
      </div>
    </TransitionGroup>
  </div>
</template>

<style scoped>
.toast-host {
  position: fixed;
  right: 16px;
  bottom: 40px;
  z-index: 4000;
  display: flex;
  flex-direction: column;
  gap: 8px;
  align-items: flex-end;
}
.toast {
  display: flex;
  align-items: center;
  gap: 8px;
  background: var(--bg-layer);
  border: 1px solid var(--stroke-card);
  border-left: 3px solid var(--accent);
  border-radius: var(--radius-md);
  box-shadow: var(--shadow-flyout);
  padding: 10px 14px;
  font-size: 13px;
  max-width: 380px;
  animation: hishell-fade-in 0.16s ease;
}
.toast.success {
  border-left-color: var(--success);
}
.toast.error {
  border-left-color: var(--danger);
}
.toast-enter-from,
.toast-leave-to {
  opacity: 0;
  transform: translateY(8px);
}
.toast-leave-active {
  transition: all 0.2s ease;
}
</style>
