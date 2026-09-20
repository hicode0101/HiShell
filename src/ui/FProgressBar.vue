<script setup lang="ts">
withDefaults(defineProps<{ value: number; error?: boolean; paused?: boolean }>(), {
  error: false,
  paused: false,
})
</script>

<template>
  <div class="f-progress" :class="{ error, paused }" role="progressbar">
    <div class="f-progress-fill" :style="{ width: Math.min(100, Math.max(0, value)) + '%' }" />
  </div>
</template>

<style scoped>
.f-progress {
  height: 4px;
  border-radius: 2px;
  background: var(--stroke-card);
  overflow: hidden;
  width: 100%;
  min-width: 80px;
}
.f-progress-fill {
  height: 100%;
  background: var(--accent);
  border-radius: 2px;
  transition: width 0.15s linear;
}
.f-progress.error .f-progress-fill {
  background: var(--danger);
}
.f-progress.paused .f-progress-fill {
  background: var(--warning);
}
</style>
